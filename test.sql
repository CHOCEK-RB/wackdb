DROP TABLE IF EXISTS shipping;
DROP TABLE IF EXISTS orders;
DROP TABLE IF EXISTS users;

CREATE TABLE users (id INTEGER, name VARCHAR, is_active BOOLEAN);
CREATE TABLE orders (id INTEGER, user_id INTEGER, amount INTEGER);
CREATE TABLE shipping (id INTEGER, order_id INTEGER, status VARCHAR);

INSERT INTO users VALUES ('not_an_int', 'Alice', true);
INSERT INTO users VALUES (4, 'MaliciousData', 'not_a_boolean');
INSERT INTO users VALUES (5, 99999, false);

INSERT INTO users VALUES (1, 'Alice', true);
INSERT INTO users VALUES (2, 'Bob', false);
INSERT INTO users VALUES (3, 'Charlie', true);
INSERT INTO users VALUES (1, 'CloneAlice', false);

INSERT INTO orders VALUES (10, 1, 150);
INSERT INTO orders VALUES (20, 2, 300);
INSERT INTO orders VALUES (30, 1, 50);

INSERT INTO shipping VALUES (100, 10, 'Entregado');
INSERT INTO shipping VALUES (200, 20, 'En Camino');
INSERT INTO shipping VALUES (300, 30, 'Procesando');
