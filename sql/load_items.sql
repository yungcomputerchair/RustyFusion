SELECT slot, id, "type", opt, timelimit
FROM inventory
WHERE playerid = $1;
