UPDATE accounts
SET
    bannedsince = $2,
    banneduntil = $3,
    banreason = $4
WHERE accountid = $1;
