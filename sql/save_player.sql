UPDATE players
SET
    "level" = $2,
    nano1 = $3,
    nano2 = $4,
    nano3 = $5,
    tutorialflag = $6,
    payzoneflag = $7,
    xcoordinate = $8,
    ycoordinate = $9,
    zcoordinate = $10,
    angle = $11,
    hp = $12,
    fusionmatter = $13,
    taros = $14,
    batteryw = $15,
    batteryn = $16,
    mentor = $17,
    currentmissionid = $18,
    warplocationflag = $19,
    skywaylocationflag = $20,
    firstuseflag = $21,
    quests = $22
WHERE playerid = $1;
