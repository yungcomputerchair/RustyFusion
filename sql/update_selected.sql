UPDATE accounts
SET
    selected = $2,
    lastlogin = $3
WHERE accountid = $1;
