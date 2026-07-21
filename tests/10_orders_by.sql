SELECT
    id,
    user_id,
    product_id
FROM
    orders
WHERE
    product_id = 16
    AND id > 17000
ORDER BY
    user_id;
