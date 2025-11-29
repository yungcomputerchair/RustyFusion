SELECT ac.AccountID, ac.AccountLevel, ac.Password, ac.Selected, ac.BannedUntil, ac.BanReason, au.Cookie, au.Expires
FROM Accounts as ac
LEFT JOIN Auth as au ON ac.AccountID = au.AccountID
WHERE ac.Login iLIKE $1
LIMIT 1;
