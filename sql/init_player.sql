INSERT INTO players (
    playerid,
    accountid,
    firstname,
    lastname,
    namecheck,
    slot,
    xcoordinate,
    ycoordinate,
    zcoordinate,
    angle,
    hp,
    skywaylocationflag,
    firstuseflag,
    quests
)
VALUES (
    $1,
    $2,
    $3,
    $4,
    $5,
    $6,
    $7,
    $8,
    $9,
    $10,
    $11,
    $12,
    $13,
    $14
);

INSERT INTO appearances (
    playerid
)
VALUES (
    $1
);
