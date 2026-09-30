#!/usr/bin/env python3
"""Bounded sustained-session and interrupted-save qualification using synthetic documents."""
import argparse, datetime, hashlib, json, os, pathlib, signal, subprocess, time

ROOT = pathlib.Path(__file__).resolve().parent.parent
parser = argparse.ArgumentParser()
parser.add_argument("evidence", type=pathlib.Path)
parser.add_argument("--revisions", type=int, default=12)
parser.add_argument("--width", type=int, default=768)
parser.add_argument("--height", type=int, default=512)
parser.add_argument("--layers", type=int, default=3)
parser.add_argument("--kill-revision", type=int, default=4)
parser.add_argument("--advanced", action="store_true", help="Qualify exact 16-bit sources/results and editable recipes")
args = parser.parse_args()
if min(args.revisions, args.width, args.height, args.layers) < 1 or not 1 <= args.kill_revision < args.revisions:
    parser.error("positive bounds and 1 <= kill-revision < revisions are required")
if args.revisions > 1000 or args.layers > 64 or args.width * args.height * args.layers > 16_000_000:
    parser.error("qualification is bounded to 1000 revisions, 64 layers and 16 million total pixels")
evidence = args.evidence.resolve()
if evidence.exists():
    raise SystemExit("Use a fresh evidence directory so stale artifacts cannot pass")
evidence.mkdir(parents=True)
env = os.environ.copy()
env["OMUSE_QUALIFICATION_ADVANCED"] = "1" if args.advanced else "0"
for key, name in (("XDG_DATA_HOME", "xdg-data"), ("XDG_CONFIG_HOME", "xdg-config"), ("XDG_CACHE_HOME", "xdg-cache"), ("XDG_STATE_HOME", "xdg-state")):
    env[key] = str(evidence / name); pathlib.Path(env[key]).mkdir()
# Resolve Cargo's reported artifact, then launch that executable directly. The
# interrupted process is the writer itself, never a Cargo wrapper or shared task.
build = subprocess.run(["cargo", "build", "--manifest-path", str(ROOT / "rust/Cargo.toml"), "--release", "--locked", "--example", "release_qualification_fixture", "--message-format=json"], cwd=ROOT, env=env, text=True, capture_output=True, timeout=600)
(evidence / "fixture-build.log").write_text(build.stderr)
if build.returncode:
    raise RuntimeError(f"fixture build failed; see {evidence / 'fixture-build.log'}")
artifacts = [json.loads(line) for line in build.stdout.splitlines() if line.startswith("{")]
executables = [entry["executable"] for entry in artifacts if entry.get("reason") == "compiler-artifact" and entry.get("target", {}).get("name") == "release_qualification_fixture" and entry.get("executable")]
if len(executables) != 1:
    raise RuntimeError("could not resolve qualification fixture executable")
base = [executables[0]]
configuration = vars(args).copy()
configuration["evidence"] = str(configuration["evidence"])
report = {"startedUtc": datetime.datetime.now(datetime.timezone.utc).isoformat(), "commit": subprocess.check_output(["git","rev-parse","HEAD"], cwd=ROOT, text=True).strip(), "configuration": configuration, "workingTreeStatus": subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT, text=True), "fixtureSha256": hashlib.sha256(pathlib.Path(base[0]).read_bytes()).hexdigest(), "harnessSourceSha256": hashlib.sha256(pathlib.Path(__file__).read_bytes()).hexdigest(), "fixtureSourceSha256": hashlib.sha256((ROOT / "rust/examples/release_qualification_fixture.rs").read_bytes()).hexdigest(), "checks": [], "limitations": ["SIGKILL timing is scheduler-dependent; integrity is asserted for whichever atomic version is visible", "This is filesystem/process qualification, not power-loss simulation"]}

def require(condition, message):
    if not condition:
        raise RuntimeError(message)

def package_hash(path):
    digest = hashlib.sha256()
    for item in sorted(p for p in path.rglob("*") if p.is_file()):
        digest.update(str(item.relative_to(path)).encode()); digest.update(item.read_bytes())
    return digest.hexdigest()

def verify(path):
    result = subprocess.run(base + ["verify", str(path)], cwd=ROOT, env=env, text=True, capture_output=True, timeout=60)
    if result.returncode: raise RuntimeError(result.stderr or result.stdout)
    record = json.loads(result.stdout.strip().splitlines()[-1])
    require((record["width"], record["height"], record["layers"]) == (args.width, args.height, args.layers), "saved dimensions or layer count differ")
    require(record["advancedLayers"] == (args.layers if args.advanced else 0), "editable layer assets were lost")
    return record

def fixture(root):
    return base + ["run", str(root), str(args.revisions), str(args.width), str(args.height), str(args.layers)]

def record(name, started, **details):
    report["checks"].append({"name":name,"seconds":round(time.monotonic()-started,3),**details})
    (evidence / "results.json").write_text(json.dumps(report, indent=2) + "\n")

started=time.monotonic()
recovery_command=["cargo","test","--manifest-path",str(ROOT/"rust/Cargo.toml"),"--release","--locked","--features","ui-test","--bin","omuse","recovery::tests::","--","--test-threads=1"]
with (evidence/"recovery-tests.log").open("w") as log:
    recovery_result=subprocess.run(recovery_command,cwd=ROOT,env=env,stdout=log,stderr=subprocess.STDOUT,timeout=600)
if recovery_result.returncode:
    raise RuntimeError(f"recovery worker tests failed; see {evidence/'recovery-tests.log'}")
record("recovery-worker",started,status="passed",command=recovery_command)

started=time.monotonic(); sustained=evidence/"sustained"
subprocess.run(fixture(sustained), cwd=ROOT, env=env, check=True,timeout=600)
journal=[json.loads(line) for line in (sustained/"journal.jsonl").read_text().splitlines()]
project=verify(sustained/"Sustained.omuse"); recovery=verify(sustained/"recovery"/"session-11111111-1111-4111-8111-111111111111.omuse")
require(len(journal)==args.revisions and project["revision"]==recovery["revision"]==args.revisions-1, "sustained revision count mismatch")
require(project["fingerprint"]==recovery["fingerprint"]==journal[-1]["projectFingerprint"]==journal[-1]["recoveryFingerprint"], "sustained pixel fingerprints differ")
record("sustained-session", started, status="passed", revisions=len(journal), project=project, sha256=package_hash(sustained/"Sustained.omuse"))

started=time.monotonic(); interrupted=evidence/"interrupted"
process=subprocess.Popen(fixture(interrupted), cwd=ROOT, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, text=True)
deadline=time.monotonic()+90; killed_phase=None
while time.monotonic()<deadline:
    phase=interrupted/"phase.json"
    if phase.exists():
        try: current=json.loads(phase.read_text())
        except (OSError,json.JSONDecodeError): current={}
        active_stages = sorted(p.name for p in interrupted.glob(".omuse-stage-*") if p.is_dir())
        if current.get("phase")=="project-saving" and current.get("revision")==args.kill_revision and active_stages:
            killed_phase={**current, "observedStages": active_stages}
            process.send_signal(signal.SIGKILL)
            break
    if process.poll() is not None: break
    time.sleep(.002)
if killed_phase is None:
    process.kill(); stderr=process.communicate()[1]; raise RuntimeError(f"did not observe interrupt window: {stderr}")
process.wait(timeout=10)
require(process.returncode == -signal.SIGKILL, "writer did not terminate from the requested signal")
project=verify(interrupted/"Sustained.omuse"); recovery=verify(interrupted/"recovery"/"session-11111111-1111-4111-8111-111111111111.omuse")
require(project["revision"] in (args.kill_revision-1,args.kill_revision), "interrupted project revision is not adjacent to killed write")
require(recovery["revision"]==args.kill_revision, "recovery did not retain the newest completed revision")
stages=sorted(p.name for p in interrupted.glob(".omuse-stage-*"))
record("interrupted-save", started, status="passed", signal="SIGKILL", observedPhase=killed_phase, visibleProject=project, recovery=recovery, projectSha256=package_hash(interrupted/"Sustained.omuse"), recoverySha256=package_hash(interrupted/"recovery"/"session-11111111-1111-4111-8111-111111111111.omuse"), orphanStages=stages)
report["completedUtc"]=datetime.datetime.now(datetime.timezone.utc).isoformat(); report["passed"]=True
(evidence/"results.json").write_text(json.dumps(report,indent=2)+"\n")
print(json.dumps(report,indent=2))
