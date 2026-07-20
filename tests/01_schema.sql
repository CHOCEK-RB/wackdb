-- E-commerce Schema
CREATE TABLE users (
    id integer,
    username varchar,
    status varchar
);

CREATE TABLE products (
    id integer,
    name varchar,
    price integer
);

CREATE TABLE orders (
    id integer,
    user_id integer,
    product_id integer
);

