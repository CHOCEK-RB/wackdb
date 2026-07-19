-- Complex WHERE to test Predicate Pushdown and OR logic
SELECT id, username, status FROM users WHERE status = 'Active' OR status = 'Banned';
