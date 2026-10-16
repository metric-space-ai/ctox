#!/usr/bin/env bash
# Isolated local coturn / Chromium acceptance; run only inside the build lane.
set -euo pipefail
: "${CARGO_TARGET_DIR:?lane target required}"
: "${TURN_NATIVE_MODULES:?existing Playwright node_modules root required}"
: "${TURN_NATIVE_CHROME:?existing Chromium path required}"
OUT="${TURN_NATIVE_OUTPUT:-$CARGO_TARGET_DIR/turn-native-proof}"
mkdir -p "$OUT"
printf '{"pass":false,"state":"running"}\n' > "$OUT/result.json"
PIDS=()
read -r TURN_PORT TLS_PORT < <(node -e 'const net=require("node:net");const a=net.createServer(),b=net.createServer();a.listen(0,"127.0.0.1",()=>b.listen(0,"127.0.0.1",()=>{console.log(a.address().port,b.address().port);a.close();b.close();}));')
export TURN_PORT TLS_PORT
cleanup() { for pid in "${PIDS[@]}"; do kill "$pid" 2>/dev/null || true; done; for pid in "${PIDS[@]}"; do wait "$pid" 2>/dev/null || true; done; }
finish() {
  rc=$?; trap - EXIT INT TERM; cleanup
  if [[ "$rc" != 0 ]]; then printf '{"pass":false,"exit":%s}\n' "$rc" > "$OUT/result.json"; fi
  exit "$rc"
}
trap finish EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
# The lane overlays preserve mtimes. A corrected source may otherwise be older
# than a prior SDK artifact, so force Cargo to inspect this producer's sources.
touch src/core/rxdb/patches/webrtc/src/peer_connection/{ice_gatherer,turn_socket,mod,driver}.rs
cargo test --locked --offline --manifest-path src/core/rxdb/Cargo.toml --test turn_native_browser --no-run
if [[ -z "${TURN_NATIVE_SERVER:-}" ]]; then
  mkdir -p "$OUT/debs" "$OUT/coturn"
  (cd "$OUT/debs"; apt-get download coturn libhiredis0.14 libmysqlclient21 libpq5 libevent-openssl-2.1-7 libevent-pthreads-2.1-7 libevent-extra-2.1-7 libevent-core-2.1-7)
  for deb in "$OUT"/debs/*.deb; do dpkg-deb -x "$deb" "$OUT/coturn"; done
  TURN_NATIVE_SERVER="$OUT/coturn/usr/bin/turnserver"
  export LD_LIBRARY_PATH="$OUT/coturn/usr/lib/x86_64-linux-gnu${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
fi
# A disposable CA is trusted only by this fixture process, never the host store.
openssl req -x509 -newkey rsa:2048 -nodes -keyout "$OUT/ca.key" -out "$OUT/ca.pem" -days 1 -subj /CN=TURN-fixture-CA >"$OUT/cert.log" 2>&1
openssl req -newkey rsa:2048 -nodes -keyout "$OUT/server.key" -out "$OUT/server.csr" -subj /CN=localhost >>"$OUT/cert.log" 2>&1
printf 'subjectAltName=DNS:localhost,IP:127.0.0.1\nextendedKeyUsage=serverAuth\n' > "$OUT/server.ext"
openssl x509 -req -in "$OUT/server.csr" -CA "$OUT/ca.pem" -CAkey "$OUT/ca.key" -CAcreateserial -out "$OUT/server.pem" -days 1 -extfile "$OUT/server.ext" >>"$OUT/cert.log" 2>&1
export SSL_CERT_FILE="$OUT/ca.pem"
"$TURN_NATIVE_SERVER" -n --use-auth-secret --static-auth-secret=turn-native-fixture-only --realm=fixture.test \
 --no-cli --no-udp --no-dtls --no-tcp-relay --allow-loopback-peers --relay-threads=1 \
 --listening-ip=127.0.0.1 --relay-ip=127.0.0.1 --listening-port="$TURN_PORT" --tls-listening-port="$TLS_PORT" \
 --min-port=49300 --max-port=49399 --max-allocate-lifetime=6 --cert="$OUT/server.pem" --pkey="$OUT/server.key" \
 --userdb="$OUT/turn.sqlite" --pidfile="$OUT/turn.pid" --log-file="$OUT/turn.log" --no-stdout-log >"$OUT/turn-stdout.log" 2>&1 &
PIDS+=("$!")
printf 'owner=turn-native-fixture pid=%s purpose=coturn output=%s stop=fixture-terminal
' "${PIDS[0]}" "$OUT" > "$OUT/processes.txt"
# Gate verifies the listener is ready, never sleeps for an assumed boot time.
node -e 'const net=require("node:net"),fs=require("node:fs"),port=Number(process.argv[1]),out=process.argv[2];const start=Date.now(),end=start+5000;let lastError=null;function check(){const s=net.connect(port,"127.0.0.1");s.once("connect",()=>{fs.writeFileSync(out+"/listener-ready.json",JSON.stringify({tcpPort:port,tlsPort:Number(process.env.TLS_PORT),ready:true,elapsedMs:Date.now()-start}));s.destroy();process.exit(0);});s.once("error",e=>{lastError=e.code;if(Date.now()>end){console.error(JSON.stringify({listenerReady:false,port,lastError}));process.exit(1);}setTimeout(check,20);});}check();' "$TURN_PORT" "$OUT"
for mode in tcp tls; do
  ROOT="$OUT/$mode"
  mkdir -p "$ROOT"
  rm -f "$ROOT/offer.json" "$ROOT/answer.json" "$ROOT/browser-result.json"
  export TURN_NATIVE_FIXTURE="$ROOT"
  node - "$ROOT" "$mode" "$TURN_NATIVE_MODULES" "$TURN_NATIVE_CHROME" <<'JS'
const fs=require('node:fs'),{createHmac}=require('node:crypto'),[root,mode,modulesRoot,chrome]=process.argv.slice(2);
const grant=seconds=>{const username=`${Math.floor(Date.now()/1000)+seconds}:fixture`;return {username,credential:createHmac('sha1','turn-native-fixture-only').update(username).digest('base64')};};
fs.writeFileSync(root+'/config.json',JSON.stringify({nativeUrl:mode==='tls'?`turns:127.0.0.1:${process.env.TLS_PORT}?transport=tcp`:`turn:127.0.0.1:${process.env.TURN_PORT}?transport=tcp`,browserUrl:`turn:127.0.0.1:${process.env.TURN_PORT}?transport=tcp`,initial:grant(8),renewed:grant(3600),modulesRoot,chrome}));
JS
  node tests/turn-native/browser.mjs "$ROOT" >"$ROOT/browser.log" 2>&1 &
  BROWSER_PID=$!; PIDS+=("$BROWSER_PID")
  printf 'owner=turn-native-fixture pid=%s purpose=chromium-controller output=%s stop=arm-terminal
' "$BROWSER_PID" "$ROOT" >> "$OUT/processes.txt"
  if cargo test --locked --offline --manifest-path src/core/rxdb/Cargo.toml --test turn_native_browser -- --ignored --nocapture >"$ROOT/native.log" 2>&1; then :; else
    rc=$?; cat "$ROOT/native.log"; exit "$rc"
  fi
  wait "$BROWSER_PID"
  PIDS=("${PIDS[0]}")
done
# Both directions are checked by exact IDs at the native receiver and browser echo.
node - "$OUT" <<'JS'
const fs=require('node:fs'),out=process.argv[2];const modes=['tcp','tls'].map(mode=>({mode,...JSON.parse(fs.readFileSync(`${out}/${mode}/browser-result.json`))}));
if(modes.some(m=>!m.pass||m.lost!==0||m.selected.relayProtocol!=='tcp'))throw Error('TURN-only gate failed');
fs.writeFileSync(out+'/result.json',JSON.stringify({pass:true,udpClientListener:false,tlsCertificateVerified:true,modes},null,2));console.log('TURN_NATIVE_PROOF '+out+'/result.json');
JS
