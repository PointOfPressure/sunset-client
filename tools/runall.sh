#!/bin/bash
# runall.sh <dir> "<test>|<test>" [target...]: verify.py on every target AT ONCE (default: sp horizon vanilla paper;
# sp = single player), each with its own game client, session folder, control socket, probe log, screenshot folder
# and Xvfb display, from one snapshot of the current build. With TWO=1 each target also gets Bot1 (CTL2; single
# player opens its world to LAN for it). Servers that are down are started first and stopped at the end; servers
# already up are left running. Logs in <dir>/<target>.log (and <dir>/<target>/ for the session's files), the table in
# <dir>/summary. On the test machine, in ~/hzmeteor; VERIFY names the test script (default verify.py), SEQ=1 runs the
# targets one after another instead (same setup, for comparing).
H=~/hzmeteor; R=$H/$1; shift
IFS='|' read -ra TESTS <<< "$1"; shift
TARGETS=("$@")
[ $# -eq 0 ] && TARGETS=(sp horizon vanilla paper)
RES=/home/sayer/horizon-proof/horizon/.local/horizon-client/26.3
mkdir -p $R
cp ${BIN:-$H/target-new/fast}/horizon-client ${BIN:-$H/target-new/fast}/horizon-ctl $R/
cp $H/${VERIFY:-verify.py} $R/verify.py

# Where Tester starts on each server (the arena is built where he stands): pinned, so a test that moves him and a
# run that stops early can't shift the next run's arena onto other terrain.
spot() {
	case "$1" in
	horizon) echo "95.5 121 158.5" ;; vanilla) echo "99.5 121 -39.5" ;; *) echo "99.5 121 -64.5" ;;
	esac
}

world() {
	case "$1" in
	horizon) echo $H/srv/world ;; vanilla) echo $H/vanilla/world ;; *) echo $H/servers/$1/world ;;
	esac
}

# launch <dir> <session file> <display> <game args...>
launch() {
	local d=$1 f=$2 disp=$3; shift 3
	local s; s=$(mktemp -d /tmp/hzrunx-XXXX); mkdir -p $s/home $s/data $s/config; echo $s > $d/$f
	(cd $s/home && HORIZON_HOME=$s/home XDG_DATA_HOME=$s/data XDG_CONFIG_HOME=$s/config HORIZON_NO_AUDIO=1 \
		HORIZON_NO_HEARTBEAT=1 HORIZON_NO_DISCORD=1 HORIZON_TELEMETRY_URL=off HORIZON_LAUNCHER_NO_SELF_UPDATE=1 \
		DISPLAY=:$disp SUNSET_PROBE=$s/probe.log \
		$R/horizon-ctl --socket $s/c.sock --timeout 300 launch --game $R/horizon-client "$@" --offscreen --wait \
		--resources $RES >/dev/null 2>&1)
}

# target <name> <display>: one target from join to quit.
target() {
	local name=$1 disp=$2 d=$R/$1 srv= at= two= t0=$SECONDS
	mkdir -p $d
	for f in session session2; do
		printf '#!/bin/sh\nexec %s/horizon-ctl --socket $(cat %s/%s)/c.sock --timeout 60 "$@"\n' $R $d $f > $d/ctl${f#session}
	done
	chmod +x $d/ctl $d/ctl2
	Xvfb :$disp -screen 0 1280x720x24 -nolisten tcp >/dev/null 2>&1 &
	local xvfb=$!
	sleep 1
	if [ $name = sp ]; then
		launch $d session $disp --world MeteorTest --creative
	else
		srv=127.0.0.1:$($H/servers.sh port $name)
		launch $d session $disp --server $srv --name Tester
	fi
	$d/ctl wait-ready 120 >/dev/null; $d/ctl wait 60 >/dev/null
	[ $name != sp ] && $d/ctl command "tp @s $(spot $name) 0 0" >/dev/null && $d/ctl wait 20 >/dev/null
	if [ "$TWO" = 1 ]; then
		at=$srv
		if [ $name = sp ]; then
			$d/ctl command "publish true" >/dev/null; sleep 3
			at=127.0.0.1:$($d/ctl chat-log 5 | grep -o "hosted on port \[[0-9]*" | grep -o "[0-9]*$" | tail -1)
		fi
		launch $d session2 $disp --server $at --name Bot1
		$d/ctl2 wait-ready 120 >/dev/null; $d/ctl2 wait 40 >/dev/null; two=$d/ctl2
	fi
	local w=; [ $name != sp ] && w=$(world $name)
	CTL2=$two SERVER=$srv WORLD=$w SHOTS=$d timeout 14400 python3 $R/verify.py $d/ctl "${TESTS[@]}" > $R/$name.log 2>&1
	[ -n "$two" ] && $d/ctl2 quit >/dev/null 2>&1
	$d/ctl quit >/dev/null 2>&1
	sleep 3
	for f in session session2; do
		[ -f $d/$f ] || continue
		local s; s=$(cat $d/$f)
		cp $s/probe.log $d/$f.probe.log 2>/dev/null
		cp $s/home/logs/latest.log $d/$f.latest.log 2>/dev/null
		rm -rf $s
	done
	kill $xvfb 2>/dev/null
	echo "$name $((SECONDS - t0))s $(tail -1 $R/$name.log)" > $d/result
}

# The servers first, all at once: a cold Spigot or Folia takes a minute or two.
started=()
for name in "${TARGETS[@]}"; do
	[ $name = sp ] && continue
	port=$($H/servers.sh port $name) || exit 2
	(echo >/dev/tcp/127.0.0.1/$port) 2>/dev/null && continue
	$H/servers.sh start $name >/dev/null; started+=($name)
done
for name in "${started[@]}"; do
	port=$($H/servers.sh port $name)
	for _ in $(seq 120); do (echo >/dev/tcp/127.0.0.1/$port) 2>/dev/null && break; sleep 2; done
done
[ ${#started[@]} -gt 0 ] && sleep 5

t0=$SECONDS
disp=101
for name in "${TARGETS[@]}"; do
	rm -f $R/$name/result
	if [ "$SEQ" = 1 ]; then target $name $disp; else target $name $disp & fi
	disp=$((disp + 1))
	sleep 2
done
wait
for name in "${started[@]}"; do $H/servers.sh stop $name >/dev/null; done
{
	for name in "${TARGETS[@]}"; do cat $R/$name/result 2>/dev/null || echo "$name no result"; done
	echo "all $((SECONDS - t0))s wall, $(date +%T)"
} | column -t > $R/summary
cat $R/summary
