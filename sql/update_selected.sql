UPDATE accounts
SET
    selected = $2
WHERE accountid = $1;
