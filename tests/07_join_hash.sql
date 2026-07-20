-- Joins orders (20k) with products (500)
-- Right table is products (<10000), so it should use HashJoin
SELECT
    id,
    user_id,
    product_id
FROM
    orders
    JOIN products ON orders.product_id = products.id
WHERE
    id < 5;

