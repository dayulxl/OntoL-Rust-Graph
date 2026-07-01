"""
P-8A 海神巡逻机 — 顺时针巡逻推演
从 Neo4j 读取 P-8A，绕圆心走 10 步闭合圆，每步更新坐标到 Neo4j，输出日志
"""
import json, math, time, urllib.request, base64

AUTH = 'Basic ' + base64.b64encode(b'neo4j:12345678').decode()
URL = 'http://localhost:7474/db/neo4j/tx/commit'

def run(cypher):
    body = json.dumps({'statements': [{'statement': cypher}]}, ensure_ascii=False).encode('utf-8')
    req = urllib.request.Request(URL, data=body, headers={'Content-Type': 'application/json'})
    req.add_header('Authorization', AUTH)
    resp = json.loads(urllib.request.urlopen(req).read())
    if resp.get('errors'):
        for e in resp['errors']: print(f'  ERROR: {e["message"]}')
        return None
    return resp['results'][0]['data']

# 1. 从 Neo4j 获取 P-8A
print("=" * 60)
print("P-8A 海神巡逻机 — 顺时针巡逻推演")
print("=" * 60)

data = run("MATCH (n:Entity {code: 'P8A_001'}) RETURN n.name, n.Space_abs, n.duration, n.code")
if not data:
    print("P-8A 不在数据库中")
    exit(1)

row = data[0]['row']
name = row[0]                # P-8A-1
pos  = row[1]                # [lat, lon, depth, height]
dur  = row[2] or 500         # 持续时间
code = row[3]

lat0, lon0, depth, height = pos[0], pos[1], pos[2], pos[3]
print(f"\n实体: {name} ({code})")
print(f"初始坐标: lat={lat0}, lon={lon0}, depth={depth}, height={height}")
print(f"持续时间: {dur}s")

# 2. 推演参数
STEPS = 10
RADIUS = 0.5  # 圆形半径（度）

print(f"\n推演配置: {STEPS} 步, 半径 {RADIUS}°, 圆心=({lat0},{lon0})")
print("-" * 60)

# 3. 顺时针走圈
for step in range(STEPS):
    # 顺时针: 角度从 0 到 -2π（或 2π 递减）
    angle = -2.0 * math.pi * step / STEPS     # 0°, -72°, -144°, ..., -360°
    lat  = round(lat0 + RADIUS * math.cos(angle), 6)
    lon  = round(lon0 + RADIUS * math.sin(angle), 6)

    now = time.strftime('%Y-%m-%dT%H:%M:%S')

    cypher = f"""
    MATCH (n:Entity {{code: '{code}'}})
    SET n.Space_abs = [{lat}, {lon}, {depth}, {height}],
        n.duration = {dur - step * 10},
        n.update_time = datetime()
    RETURN n.Space_abs AS pos, n.duration AS remaining
    """
    result = run(cypher)
    if result:
        new_pos = result[0]['row'][0]
        rem     = result[0]['row'][1]
        dir_label = chr(8594 + (step % 4))  # →↗↑↖←↙↓↘  approximate
        print(f"  Step {step+1:2d}/10  [{now}]  "
              f"lat={new_pos[0]}, lon={new_pos[1]}  "
              f"剩余={rem}s")

print("-" * 60)

# 4. 最后一步应该回到原点
data = run(f"MATCH (n:Entity {{code: '{code}'}}) RETURN n.Space_abs, n.duration")
final_pos  = data[0]['row'][0]
final_dur  = data[0]['row'][1]
print(f"\n最终位置: lat={final_pos[0]}, lon={final_pos[1]}")
print(f"起点:     lat={lat0}, lon={lon0}")
dist = math.sqrt((final_pos[0]-lat0)**2 + (final_pos[1]-lon0)**2)
print(f"偏移:     {dist:.6f}°")
if dist < 0.01:
    print("✅ 成功回到原点，航线闭合")
else:
    print("⚠ 未精确回到原点（浮点误差）")

print(f"\n总耗时: {500 - final_dur}s")
print("=" * 60)
