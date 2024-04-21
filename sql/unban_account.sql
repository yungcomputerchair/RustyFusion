UPDATE accounts
SET
    bannedsince = 0,
    banneduntil = 0,
    banreason = ''
WHERE accountid = $1;
