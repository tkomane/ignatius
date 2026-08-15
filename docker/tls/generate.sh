#!/bin/sh
# Generates a throwaway certificate authority, a server certificate and a client
# certificate for the TLS development database.
#
# Everything here is disposable and regenerated on demand. None of it is
# committed: certificates and keys have no business in a repository, and these
# are worthless outside a container on localhost anyway.
set -e
dir="$(cd "$(dirname "$0")" && pwd)/generated"
mkdir -p "$dir"

if [ -f "$dir/server.crt" ] && [ -f "$dir/client.crt" ]; then
    exit 0
fi

# A certificate authority that exists only for these tests.
openssl req -new -x509 -days 365 -nodes -sha256 \
    -subj "/CN=Ignatius development CA" \
    -keyout "$dir/ca.key" -out "$dir/ca.crt" 2>/dev/null

# The server certificate. The name matters, and so does what is missing from it:
# the certificate covers "localhost" and deliberately not "127.0.0.1", so the
# same server proves both cases. Connecting by name satisfies verify-full;
# connecting by address fails it and passes verify-ca, which is exactly the
# difference between those two modes.
openssl req -new -nodes -sha256 \
    -subj "/CN=localhost" \
    -keyout "$dir/server.key" -out "$dir/server.csr" 2>/dev/null
printf 'subjectAltName=DNS:localhost\n' > "$dir/server.ext"
openssl x509 -req -in "$dir/server.csr" -days 365 -sha256 \
    -CA "$dir/ca.crt" -CAkey "$dir/ca.key" -CAcreateserial \
    -extfile "$dir/server.ext" -out "$dir/server.crt" 2>/dev/null

# A client certificate, so certificate authentication can be tested. Its common
# name is the role it authenticates as.
openssl req -new -nodes -sha256 \
    -subj "/CN=cert_user" \
    -keyout "$dir/client.key" -out "$dir/client.csr" 2>/dev/null
# The extensions are not optional. Signing without any of them produces an
# X.509 v1 certificate on some OpenSSL versions, which rustls rejects outright
# as UnsupportedCertVersion. Naming them forces v3 everywhere.
cat > "$dir/client.ext" <<'EXT'
basicConstraints=CA:FALSE
keyUsage=digitalSignature,keyEncipherment
extendedKeyUsage=clientAuth
EXT
openssl x509 -req -in "$dir/client.csr" -days 365 -sha256 \
    -CA "$dir/ca.crt" -CAkey "$dir/ca.key" -CAcreateserial \
    -extfile "$dir/client.ext" -out "$dir/client.crt" 2>/dev/null

# PostgreSQL refuses to start if its key is readable by anyone else, and the
# container runs as uid 70 in the alpine image.
chmod 600 "$dir/server.key" "$dir/client.key" "$dir/ca.key"
rm -f "$dir"/*.csr "$dir"/*.ext
