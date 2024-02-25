SELECT taskid, remainingnpccount1, remainingnpccount2, remainingnpccount3
FROM runningquests
WHERE playerid = $1;
