SELECT
    p.PlayerID, p.Slot, p.FirstName, p.LastName, p.Level, p.AppearanceFlag, p.TutorialFlag, p.PayZoneFlag,
    p.XCoordinate, p.YCoordinate, p.ZCoordinate, p.NameCheck,
    a.Body, a.EyeColor, a.FaceStyle, a.Gender, a.HairColor, a.HairStyle, a.Height, a.SkinColor
FROM Players as p
INNER JOIN Appearances as a ON p.PlayerID = a.PlayerID
WHERE p.AccountID = $1;
