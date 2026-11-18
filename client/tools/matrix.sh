#!/bin/bash
# The test matrix: the whole of verify.py on each test server in turn (servers.sh): start it, join it as Tester, run
# every test, keep the log in ~/hzmeteor/matrix/<server>.log, stop it. tools/matrix_report.py then makes the table
# (docs/test-matrix.md). On the test machine, in ~/hzmeteor, with Sunset built with the probe staged next to it:
#   ./matrix.sh [server...]      (default: horizon vanilla paper purpur folia spigot)
H=~/hzmeteor
cd $H || exit 1
mkdir -p matrix

world() {
	case "$1" in
	horizon) echo $H/srv/world ;; vanilla) echo $H/vanilla/world ;; *) echo $H/servers/$1/world ;;
	esac
}

for name in ${@:-horizon vanilla paper purpur folia spigot}; do
	port=$(./servers.sh port "$name") || continue
	./ctl quit >/dev/null 2>&1
	./servers.sh start "$name"
	# Up once it takes connections (Spigot and Folia take longest).
	for _ in $(seq 120); do
		(echo >/dev/tcp/127.0.0.1/$port) 2>/dev/null && break
		sleep 2
	done
	sleep 5
	if ./run-join.sh 127.0.0.1:$port >/dev/null && ./ctl wait-ready 120 >/dev/null && ./ctl wait 60 >/dev/null; then
		echo "$(date '+%F %T') $name: started" >&2
		SERVER=127.0.0.1:$port WORLD=$(world "$name") timeout 21600 python3 verify.py $H/ctl > matrix/$name.log 2>&1
	else
		echo "could not join $name on port $port" > matrix/$name.log
	fi
	./ctl quit >/dev/null 2>&1
	./servers.sh stop "$name"
	echo "$(date '+%F %T') $name: $(tail -1 matrix/$name.log)" >&2
	sleep 5
done
