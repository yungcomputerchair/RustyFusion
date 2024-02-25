SELECT
    p.PlayerID, p.Slot, p.FirstName, p.LastName, p.NameCheck,
    p.Level, p.HP, p.FusionMatter, p.Taros, p.BatteryW, p.BatteryN,
    p.AppearanceFlag, p.TutorialFlag, p.PayZoneFlag, p.FirstUseFlag,
    p.WarpLocationFlag, p.SkywayLocationFlag,
    p.CurrentMissionID, p.Quests,
    p.XCoordinate, p.YCoordinate, p.ZCoordinate, p.Angle,
    p.Nano1, p.Nano2, p.Nano3,
    a.Body, a.EyeColor, a.FaceStyle, a.Gender, a.HairColor, a.HairStyle, a.Height, a.SkinColor
FROM Players as p
INNER JOIN Appearances as a ON p.PlayerID = a.PlayerID
WHERE p.AccountID = $1;
