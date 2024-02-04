SELECT AccountID, AccountLevel, Password, Selected, BannedUntil, BanReason
FROM Accounts
WHERE Login iLIKE $1
LIMIT 1;
