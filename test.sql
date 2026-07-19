DROP TABLE IF EXISTS shipping;

DROP TABLE IF EXISTS orders;

DROP TABLE IF EXISTS users;

CREATE TABLE users (
    id integer,
    name varchar,
    is_active boolean
);

CREATE TABLE orders (
    id integer,
    user_id integer,
    amount integer
);

CREATE TABLE shipping (
    id integer,
    order_id integer,
    status varchar
);

INSERT INTO users
    VALUES ('not_an_int', 'Alice', TRUE);

INSERT INTO users
    VALUES (4, 'MaliciousData', 'not_a_boolean');

INSERT INTO users
    VALUES (5, 99999, FALSE);

INSERT INTO users
    VALUES (1, 'Alice', TRUE);

INSERT INTO users
    VALUES (2, 'Bob', FALSE);

INSERT INTO users
    VALUES (3, 'Charlie', TRUE);

INSERT INTO users
    VALUES (1, 'CloneAlice', FALSE);

INSERT INTO orders
    VALUES (10, 1, 150);

INSERT INTO orders
    VALUES (20, 2, 300);

INSERT INTO orders
    VALUES (30, 1, 50);

INSERT INTO shipping
    VALUES (100, 10, 'Entregado');

INSERT INTO shipping
    VALUES (200, 20, 'En Camino');

INSERT INTO shipping
    VALUES (300, 30, 'Procesando');

