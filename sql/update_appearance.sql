UPDATE appearances
SET
    body = $2,
    eyecolor = $3,
    facestyle = $4,
    gender = $5,
    haircolor = $6,
    hairstyle = $7,
    height = $8,
    skincolor = $9
WHERE playerid = $1;

UPDATE players
SET
    appearanceflag = $2
WHERE playerid = $1;
