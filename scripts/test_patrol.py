"""测试 POST /patrol — 验证日志输出"""
import subprocess, time, json, urllib.request, base64, sys, threading

AUTH = 'Basic ' + base64.b64encode(b'neo4j:12345678').decode()
URL = 'http://localhost:8085/patrol'

# 启动服务器
print("Starting server...")
server = subprocess.Popen(
    ["target/debug/ontology-server.exe"],
    cwd="e:/Rust/ontologica_rust_graph",
    stderr=subprocess.PIPE,
    stdout=subprocess.PIPE,
    text=True,
    encoding="utf-8",
    errors="replace",
)

# 等待就绪
for _ in range(20):
    try:
        urllib.request.urlopen("http://localhost:8085/health").read()
        print("Server ready.")
        break
    except:
        time.sleep(0.5)
else:
    print("Server didn't start!")
    server.kill()
    sys.exit(1)

# 发送 POST
body = json.dumps([{
    "id": "test-patrol-1",
    "code": "TEST_PATROL_01",
    "name": "测试巡逻",
    "waypoints": [
        {"seq": 1, "lat": 32.0, "lon": 118.0, "alt": 500, "action": "HOVER"},
        {"seq": 2, "lat": 32.1, "lon": 118.1, "alt": 500, "action": "SCAN"},
    ]
}], ensure_ascii=False).encode('utf-8')

print("\n=== Sending POST /patrol ===")
req = urllib.request.Request(URL, data=body, headers={'Content-Type': 'application/json; charset=utf-8'})
req.add_header('Authorization', AUTH)
resp = urllib.request.urlopen(req)
resp_body = resp.read().decode('utf-8')
print(f"HTTP {resp.status}")
print(resp_body[:500])

# 等日志
time.sleep(1)

# 读取 stderr
server.kill()
stdout, stderr = server.communicate(timeout=2)

print("\n=== STDOUT ===")
print(stdout[-500:] if stdout else "(empty)")

print("\n=== STDERR (LOGS) ===")
print(stderr[-2000:] if stderr else "(empty)")
