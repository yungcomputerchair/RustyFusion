UPDATE Auth
SET Expires = 0
WHERE AccountID = $1;
