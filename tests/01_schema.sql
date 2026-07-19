-- E-commerce Schema
CREATE TABLE users (id Integer, username Varchar, status Varchar);
CREATE TABLE products (id Integer, name Varchar, price Integer);
CREATE TABLE orders (id Integer, user_id Integer, product_id Integer);
