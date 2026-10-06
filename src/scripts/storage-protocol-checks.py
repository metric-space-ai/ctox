"""Bounded real SFTP/SMB3 fixtures; run only inside the gpu-build-run lane."""
import base64, hashlib, json, os, pathlib, secrets, signal, socket, subprocess, time, uuid
root=pathlib.Path.cwd()
artifacts=pathlib.Path('/mnt/nvme1/build-lane/artifacts/ctox-storage')
assert root==pathlib.Path('/mnt/nvme1/build-lane/src/ctox-storage')
assert os.environ['CARGO_TARGET_DIR'].startswith('/mnt/nvme1/build-lane/target/')
os.umask(0o077)
fixture=artifacts/('protocol-'+uuid.uuid4().hex[:12]);fixture.mkdir(parents=True)
name='ctox-storage-'+uuid.uuid4().hex[:12]
receipt={'owner':'01a0e259-5815-7f72-aa87-31db2f872425','pid':os.getpid(),'container':name,'directory':str(fixture),'stop_condition':'all protocol checks finish or 900s deadline','started_at':time.time()}
receipt_path=fixture/'receipt.json'
receipt_path.write_text(json.dumps(receipt,indent=2))
def interrupted(signum,frame): raise KeyboardInterrupt()
signal.signal(signal.SIGTERM,interrupted)
def run(command,timeout=120,input=None):
 with (fixture/'commands.log').open('a') as output:
  return subprocess.run(command,check=True,stdout=output,stderr=subprocess.STDOUT,timeout=timeout,input=input,text=True)
recipe='''FROM ubuntu:24.04
RUN apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends openssh-server samba && rm -rf /var/lib/apt/lists/*
CMD ["/bin/sh","/fixture/start.sh"]
'''
tag='ctox-storage-protocol:'+hashlib.sha256(recipe.encode()).hexdigest()[:12]
try:
 (fixture/'Dockerfile').write_text(recipe)
 if subprocess.run(['docker','image','inspect',tag],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL).returncode:
  print('PROTOCOL_PHASE: build fixture image',flush=True)
  run(['docker','build','--network','default','--tag',tag,str(fixture)],timeout=420)
 for key in ['client','host']:
  run(['ssh-keygen','-q','-t','ed25519','-N','','-f',str(fixture/key)])
 for directory in ['ssh-data','smb-data']:
  (fixture/directory).mkdir()
 password=secrets.token_urlsafe(32)
 (fixture/'password').write_text(password+'\n'+password+'\n')
 (fixture/'sshd_config').write_text('''Port 22
ListenAddress 0.0.0.0
HostKey /fixture/host
AuthorizedKeysFile /fixture/client.pub
StrictModes no
PasswordAuthentication no
KbdInteractiveAuthentication no
UsePAM no
PermitRootLogin no
AllowUsers storage
Subsystem sftp internal-sftp
''')
 (fixture/'smb.conf').write_text('''[global]
server role = standalone server
server min protocol = SMB3_00
smb encrypt = required
map to guest = Never
load printers = no
printing = bsd
printcap name = /dev/null
[artifacts]
path = /fixture/smb-data
read only = no
valid users = storage
follow symlinks = no
wide links = no
''')
 (fixture/'start.sh').write_text('''set -eu
mkdir -p /run/sshd
useradd -o -u 1000 -M -s /bin/sh storage
printf 'storage:%s\\n' "$(head -n 1 /fixture/password)" | chpasswd
smbpasswd -s -a storage < /fixture/password
chown storage /fixture/ssh-data /fixture/smb-data
/usr/sbin/sshd -D -e -f /fixture/sshd_config &
exec /usr/sbin/smbd --foreground --no-process-group --configfile=/fixture/smb.conf
''')
 run(['docker','run','--detach','--name',name,'--label','ctox.owner='+receipt['owner'],'--cpus','2','--memory','768m','--pids-limit','128','--publish','127.0.0.1::22','--publish','127.0.0.1::445','--mount','type=bind,source='+str(fixture)+',target=/fixture',tag])
 inspect=json.loads(subprocess.check_output(['docker','inspect',name],text=True))[0]
 ports=inspect['NetworkSettings']['Ports']
 mapped={kind:int(ports[port][0]['HostPort']) for kind,port in [('ssh','22/tcp'),('smb','445/tcp')]}
 receipt.update(container_id=inspect['Id'],ports=mapped)
 receipt_path.write_text(json.dumps(receipt,indent=2))
 deadline=time.monotonic()+40
 for port in mapped.values():
  while True:
   try:
    with socket.create_connection(('127.0.0.1',port),timeout=1): break
   except OSError:
    assert time.monotonic()<deadline,'fixture did not start'
    time.sleep(.25)
 public=(fixture/'host.pub').read_text().split()[1]
 pin='SHA256:'+base64.b64encode(hashlib.sha256(base64.b64decode(public)).digest()).decode().rstrip('=')
 for protocol in ['ssh','smb']:
  config={'host_root':str(fixture/'ssh-data'),'protocol':protocol,'host':'127.0.0.1','port':mapped[protocol],'username':'storage','root':'/fixture/ssh-data' if protocol=='ssh' else '/','share':'artifacts','password':password,'private_key':(fixture/'client').read_text(),'host_key_sha256':pin}
  config_path=fixture/(protocol+'.json');config_path.write_text(json.dumps(config))
  env=os.environ.copy();env['STORAGE_LIVE_CONFIG']=str(config_path)
  print('PROTOCOL_PHASE: '+protocol,flush=True)
  with (fixture/(protocol+'-test.log')).open('w') as output:
   subprocess.run(['cargo','test','--locked','--manifest-path','src/core/transfers/Cargo.toml','--test','storage_live','-j','6','--','--ignored','--nocapture','--test-threads=1'],check=True,env=env,stdout=output,stderr=subprocess.STDOUT,timeout=180)
  receipt[protocol]='passed'
  print('PROTOCOL_PASS: '+protocol,flush=True)
 receipt['result']='passed'
finally:
 with (fixture/'server.log').open('w') as output:
  subprocess.run(['docker','logs',name],stdout=output,stderr=subprocess.STDOUT,timeout=15)
 removed=subprocess.run(['docker','rm','--force',name],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,timeout=30)
 receipt['container_removed']=removed.returncode==0
 receipt['finished_at']=time.time()
 receipt_path.write_text(json.dumps(receipt,indent=2))
 print('PROTOCOL_RECEIPT: '+str(receipt_path),flush=True)
