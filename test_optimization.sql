CREATE TABLE roles (
    id integer,
    role_name varchar
);

CREATE TABLE users (
    id integer,
    username varchar,
    role_id integer
);

INSERT INTO roles
    VALUES (1, 'Admin');

INSERT INTO roles
    VALUES (2, 'User');

INSERT INTO users
    VALUES (101, 'alice', 1);

INSERT INTO users
    VALUES (102, 'bob', 2);

INSERT INTO users
    VALUES (103, 'charlie', 2);

INSERT INTO users
    VALUES (104, 'david', 1);

SELECT id, username, role_name FROM users JOIN roles ON role_id = id WHERE id > 101;

