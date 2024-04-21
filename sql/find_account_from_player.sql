SELECT a.AccountID, a.AccountLevel, a.Login, a.Password, a.Selected, a.BannedUntil, a.BanReason
FROM Accounts as a
INNER JOIN Players as p ON p.AccountID = a.AccountID
WHERE p.PlayerID = $1;
