SELECT
    p.PlayerID, p.FirstName, p.LastName, p.NameCheck, p.AppearanceFlag,
    s.Body, s.EyeColor, s.FaceStyle, s.Gender, s.HairColor, s.HairStyle, s.Height, s.SkinColor
FROM Players as p
INNER JOIN Appearances as s ON p.PlayerID = s.PlayerID
WHERE p.PlayerID = $1;
