-- Joins products (500) with users (15k)
-- Right table is users (>10000), so it should use NestedLoopJoin
SELECT id, name, price FROM products JOIN users ON id = id WHERE id < 2;
