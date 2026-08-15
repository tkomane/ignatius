-- A role that authenticates with a client certificate rather than a password.
-- Harmless on the plain server, where the certificate path is never taken.
CREATE ROLE cert_user LOGIN;
GRANT CONNECT ON DATABASE ignatius_demo TO cert_user;
GRANT USAGE ON SCHEMA public TO cert_user;
GRANT SELECT ON orders TO cert_user;
