#!/bin/bash
# The test servers on the test machine, each in ~/hzmeteor/servers/<name> on its own port, offline mode, stock
# configs, Tester and Bot1 operators:
#   servers.sh setup <name>     download (or build) the jar and write the folder
#   servers.sh start|stop|log <name>
#   servers.sh port <name>
# Names: horizon (Horizon's own dedicated server, ~/hzmeteor/srv), vanilla (~/hzmeteor/vanilla), paper, purpur,
# folia, spigot (built once with BuildTools).
set -e
H=~/hzmeteor
JAVA=/usr/lib/jvm/java-25-openjdk-amd64/bin/java
name=$2

port() {
	case "$1" in
	horizon) echo 25599 ;; vanilla) echo 25598 ;; paper) echo 25601 ;; purpur) echo 25602 ;;
	folia) echo 25603 ;; spigot) echo 25604 ;; *) echo "unknown server $1" >&2; exit 2 ;;
	esac
}

dir() {
	case "$1" in
	horizon) echo $H/srv ;; vanilla) echo $H/vanilla ;; *) echo $H/servers/$1 ;;
	esac
}

# Offline-mode UUIDs (UUID v3 of "OfflinePlayer:<name>"), so Tester and Bot1 are operators wherever they join.
ops() {
	python3 - "$1" <<'EOF'
import hashlib, json, sys, uuid
def offline(name):
    h = bytearray(hashlib.md5(f"OfflinePlayer:{name}".encode()).digest())
    h[6] = (h[6] & 0x0F) | 0x30
    h[8] = (h[8] & 0x3F) | 0x80
    return str(uuid.UUID(bytes=bytes(h)))
json.dump([{"uuid": offline(n), "name": n, "level": 4, "bypassesPlayerLimit": True} for n in ("Tester", "Bot1")],
          open(sys.argv[1] + "/ops.json", "w"), indent=2)
EOF
}

fill_jar() {
	# The newest build of a PaperMC project for 26.3 (stable if there is one).
	curl -sf "https://fill.papermc.io/v3/projects/$1/versions/26.3/builds" | python3 -c '
import json, sys
builds = json.load(sys.stdin)
stable = [b for b in builds if b["channel"] == "STABLE"]
print((stable or builds)[0]["downloads"]["server:default"]["url"])'
}

setup() {
	local d; d=$(dir "$1")
	mkdir -p "$d"
	cd "$d"
	case "$1" in
	paper | folia) curl -sfo server.jar "$(fill_jar "$1")" ;;
	purpur) curl -sfo server.jar https://api.purpurmc.org/v2/purpur/26.3/latest/download ;;
	spigot)
		mkdir -p build && cd build
		curl -sfo BuildTools.jar https://hub.spigotmc.org/jenkins/job/BuildTools/lastSuccessfulBuild/artifact/target/BuildTools.jar
		nice -n 19 $JAVA -jar BuildTools.jar --rev 26.3 > buildtools.log 2>&1
		cp spigot-26.3.jar ../server.jar
		cd ..
		;;
	horizon | vanilla) ;;
	esac
	echo "eula=true" > eula.txt
	# The vanilla folder's settings (seed, creative, no spawn protection, offline) on this server's port.
	sed "s/^server-port=.*/server-port=$(port "$1")/" $H/vanilla/server.properties > server.properties
	ops "$d"
	echo "set up $1 in $d"
}

start() {
	local d; d=$(dir "$1")
	cd "$d"
	case "$1" in
	horizon) $H/server.sh start ;;
	vanilla) $H/vanilla.sh start ;;
	*)
		pkill -f "[s]ervers/$1/server.jar" 2>/dev/null || true
		sleep 1
		nohup $JAVA -Xms1G -Xmx3G -jar "$d/server.jar" --nogui < /dev/null > "$d/out.log" 2>&1 &
		echo started
		;;
	esac
}

stop() {
	case "$1" in
	horizon) $H/server.sh stop ;;
	vanilla) $H/vanilla.sh stop ;;
	*) pkill -f "[s]ervers/$1/server.jar" && echo stopped || true ;;
	esac
}

case "$1" in
setup) setup "$name" ;;
start) start "$name" ;;
stop) stop "$name" ;;
log) tail -"${3:-20}" "$(dir "$name")/out.log" ;;
port) port "$name" ;;
*) echo "usage: servers.sh setup|start|stop|log|port <name>" >&2; exit 2 ;;
esac
