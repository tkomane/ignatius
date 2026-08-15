-- Synthetic demo data. No real customer, account, or personal data appears here
-- and none ever should: the test suite must never need a real database.

CREATE TABLE orders (
    order_id    bigserial PRIMARY KEY,
    customer_id integer     NOT NULL,
    total       numeric(12, 2) NOT NULL,
    currency    text        NOT NULL DEFAULT 'ZAR',
    note        text,
    created_at  timestamptz NOT NULL DEFAULT now()
);

INSERT INTO orders (customer_id, total, note, created_at) VALUES
    (10482, 1245.00, 'standard delivery', '2026-08-14 08:14:22+02'),
    (10483,   89.99, NULL,                '2026-08-14 09:02:11+02'),
    (10484, 15300.75, 'bulk order',       '2026-08-15 06:45:00+02');

CREATE VIEW recent_orders AS
    SELECT order_id, customer_id, total, created_at
    FROM orders
    WHERE created_at >= current_date - 7;

-- A table exercising the value-rendering rules: NULL against empty string,
-- wide characters, and text that would drive a terminal if it were not escaped.
CREATE TABLE rendering_cases (
    label       text PRIMARY KEY,
    value       text
);

INSERT INTO rendering_cases (label, value) VALUES
    ('sql_null',      NULL),
    ('empty_string',  ''),
    ('literal_null',  'NULL'),
    ('wide_chars',    '日本語のテキスト'),
    ('combining',     'e' || U&'\0301'),
    ('escape_attack', E'\x1b[2J\x1b[31mnot a real prompt\x1b[0m'),
    ('tab_and_lf',    E'before\tafter\nsecond line');

CREATE TABLE type_coverage (
    as_int      integer,
    as_bigint   bigint,
    as_numeric  numeric(20, 8),
    as_bool     boolean,
    as_uuid     uuid,
    as_json     jsonb,
    as_array    integer[],
    as_bytea    bytea,
    as_interval interval,
    as_inet     inet,
    as_range    int4range,
    as_ts       timestamptz
);

INSERT INTO type_coverage VALUES (
    42,
    9223372036854775807,
    123456789.12345678,
    true,
    '3f2504e0-4f89-11d3-9a0c-0305e82c3301',
    '{"nested": {"a": [1, 2, 3]}}',
    '{1,2,3}',
    '\xdeadbeef',
    '1 year 2 mons 3 days 04:05:06',
    '192.168.0.1/24',
    '[1,10)',
    '2026-08-15 12:00:00+02'
);

-- A restricted role, so metadata behaviour under limited permissions is testable.
CREATE ROLE restricted_reader LOGIN PASSWORD 'not-a-real-password-restricted';
GRANT CONNECT ON DATABASE ignatius_demo TO restricted_reader;
GRANT USAGE ON SCHEMA public TO restricted_reader;
GRANT SELECT ON orders TO restricted_reader;

-- A relation whose name would break any client that interpolates identifiers
-- instead of binding them. It exists so the object tree and the SQL-quoting
-- helper are tested against the shape an attacker would actually use.
CREATE TABLE "we""ird ""; DROP TABLE orders; --" (
    id integer PRIMARY KEY,
    note text
);

INSERT INTO "we""ird ""; DROP TABLE orders; --" VALUES (1, 'still here');

CREATE SCHEMA reporting;
CREATE VIEW reporting.order_totals AS
    SELECT customer_id, sum(total) AS lifetime_total
    FROM orders GROUP BY customer_id;

CREATE FUNCTION reporting.order_count() RETURNS bigint
    LANGUAGE sql STABLE AS $$ SELECT count(*) FROM orders $$;

-- A table the restricted role can see in the catalogue but not read, so the
-- tree can be tested showing an object it must mark as unreadable.
CREATE TABLE secrets_of_the_realm (id integer PRIMARY KEY, value text);
