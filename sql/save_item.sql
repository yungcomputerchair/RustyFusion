INSERT INTO inventory (
    playerid,
    slot,
    id,
    "type",
    opt,
    timelimit
)
VALUES (
    $1,
    $2,
    $3,
    $4,
    $5,
    $6
);
