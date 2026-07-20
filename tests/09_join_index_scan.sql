-- Joins users with orders
-- The where clause on users.id (< 10) triggers an IndexScan with range bounds
SELECT users.username, orders.product_id FROM users
JOIN orders ON users.id = orders.user_id
WHERE users.id < 10;
