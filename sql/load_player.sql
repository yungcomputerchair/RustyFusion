SELECT
    p.PlayerID, p.Slot, p.FirstName, p.LastName, p.NameCheck,
    p.Level, p.HP, p.FusionMatter, p.Taros, p.BatteryW, p.BatteryN,
    p.AppearanceFlag, p.TutorialFlag, p.PayZoneFlag, p.FirstUseFlag, p.Mentor,
    p.WarpLocationFlag, p.SkywayLocationFlag,
    p.CurrentMissionID, p.Quests,
    p.XCoordinate, p.YCoordinate, p.ZCoordinate, p.Angle,
    p.Nano1, p.Nano2, p.Nano3,
    s.Body, s.EyeColor, s.FaceStyle, s.Gender, s.HairColor, s.HairStyle, s.Height, s.SkinColor,
    a.AccountLevel
FROM Players as p
INNER JOIN Appearances as s ON p.PlayerID = s.PlayerID
INNER JOIN Accounts as a ON p.AccountID = a.AccountID
WHERE p.PlayerID = $1;
