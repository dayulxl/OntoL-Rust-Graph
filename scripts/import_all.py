"""Anti-Submarine Warfare 知识图谱导入 — 无 APOC 依赖"""
import json, urllib.request, base64

AUTH = 'Basic ' + base64.b64encode(b'neo4j:12345678').decode()
URL = 'http://localhost:7474/db/neo4j/tx/commit'

def run(cypher_or_list):
    if isinstance(cypher_or_list, list):
        stmts = [{'statement': s} for s in cypher_or_list]
    else:
        stmts = [{'statement': cypher_or_list}]
    body = json.dumps({'statements': stmts}, ensure_ascii=False).encode('utf-8')
    req = urllib.request.Request(URL, data=body, headers={'Content-Type': 'application/json; charset=utf-8'})
    req.add_header('Authorization', AUTH)
    resp = json.loads(urllib.request.urlopen(req).read())
    if resp.get('errors'):
        for e in resp['errors']:
            print(f'  ERROR: {e["message"]}')
        return False
    return True

# ── Step 1: Constraints ──
print("1. Creating constraints...")
run([
    "CREATE CONSTRAINT entity_id_constraint IF NOT EXISTS FOR (e:Entity) REQUIRE e.id IS UNIQUE",
    "CREATE CONSTRAINT entity_code_constraint IF NOT EXISTS FOR (e:Entity) REQUIRE e.code IS UNIQUE",
])
print("   OK")

# ── Step 2: Import entities (one MERGE per statement, batched in groups) ──
print("2. Importing entities...")

BASE = """n.id = randomUUID(), n.graph_id = randomUUID(),
  n.domain = 'Anti-SubmarineWarfare', n.leven = 1,
  n.update_time = datetime(), n.create_time = datetime(),
  n.confidence = 1.0, n.status = '有效', n.version = '1.0',
  n.cope_version = '1.0', n.source = 'OWL_Import', n.owner = 'Admin'"""

def make_cypher(code, name, etype, desc='', **extra):
    extra_sets = ''.join(f', n.`{k}` = {json.dumps(v)}' for k, v in extra.items())
    return f"""MERGE (n:Entity {{code: '{code}'}})
ON CREATE SET n.name = '{name}', {BASE}
SET n.type = '{etype}', n.description = '{desc}'{extra_sets}"""

stmts = []

# P-8A
stmts.append(make_cypher('P8A_001', 'P-8A-1', 'P-8A海神巡逻机',
    作战半径=2222, 实用升限=12500, 巡航速度=815, 最大航程=8300,
    最大起飞重量_kg_=85820, 最大速度=907, 机长=39.47, 机高=12.83, 续航时间=10, 翼展=37.64))

# MH-60R ×10
for i in range(1, 11):
    stmts.append(make_cypher(f'MH60R_{i:03d}', f'MH-60R-{i}', 'MH-60R海鹰反潜直升机'))

# MQ-4C ×6
for i in range(1, 7):
    stmts.append(make_cypher(f'MQ4C_{i:03d}', f'MQ-4C-{i}', 'MQ-4C人鱼海神无人机'))

# MK-54 ×8
for i in range(1, 9):
    stmts.append(make_cypher(f'MK54_{i:03d}', f'MK-54-{i}', 'Mk_54_轻型反潜鱼雷',
        射程=10, 战斗部=44.5, 深度=450, 速度=40))

# Sensors
stmts.append(make_cypher('BT_001', 'BT-1', '海洋测温浮标', '海洋测温浮标'))
stmts.append(make_cypher('DICASS_001', 'DICASS-1', '主动声纳浮标', '主动定向浮标'))
stmts.append(make_cypher('DIFAR_001', 'DIFAR-1', '被动声纳浮标', '被动定向浮标'))
stmts.append(make_cypher('FLIR_001', 'FLIR-1', '雷达', '前视红外系统'))
stmts.append(make_cypher('MAD_001', 'MAD-XR-1', '磁异探测', '磁异探测仪'))
stmts.append(make_cypher('SATCOM_001', 'WEX-TX-1', '卫星通讯', '卫星通讯'))
stmts.append(make_cypher('RADIO_001', 'WX-TX-1', '无线通讯', '无线通讯'))

# Ships
ships = [
    ('CVN68', 'CVN-68_尼米兹号', '尼米兹级', '尼米兹级航母'),
    ('CVN69', 'CVN-69_艾森豪威尔号', '尼米兹级', '尼米兹级航母'),
    ('CVN70', 'CVN-70_卡尔·文森号', '尼米兹级', '尼米兹级航母'),
    ('CVN71', 'CVN-71_西奥多·罗斯福号', '尼米兹级', '尼米兹级航母'),
    ('CVN72', 'CVN-72_亚伯拉罕·林肯号', '尼米兹级', '尼米兹级航母'),
    ('CVN73', 'CVN-73_乔治·华盛顿号', '尼米兹级', '尼米兹级航母'),
    ('CVN74', 'CVN-74_约翰·C·斯坦尼斯号', '尼米兹级', '尼米兹级航母'),
    ('CVN75', 'CVN-75_哈里·S·杜鲁门号', '尼米兹级', '尼米兹级航母'),
    ('CVN76', 'CVN-76_罗纳德·里根号', '尼米兹级', '尼米兹级航母'),
    ('CVN77', 'CVN-77_乔治·H·W·布什号', '尼米兹级', '尼米兹级航母'),
    ('CVN78', 'CVN-78_杰拉尔德·R·福特号', '福特级', '福特级航母'),
    ('CG59', 'CG-59普林斯顿号', '提康德罗加级', '提康德罗加级巡洋舰'),
    ('DDG125', 'DDG-125杰克·H·卢卡斯号', '阿利伯克级', '阿利伯克级驱逐舰'),
    ('DDG128', 'DDG-128泰德·史蒂文斯号', '阿利伯克级', '阿利伯克级驱逐舰'),
    ('DDG129', 'DDG-129杰里迈亚·丹顿号', '阿利伯克级', '阿利伯克级驱逐舰'),
    ('DDG131', 'DDG-131乔治·M·尼尔号', '阿利伯克级', '阿利伯克级驱逐舰'),
    ('TAOE6', 'T-AOE-6_供应号', '供应级', '供应级补给舰'),
]
for code, name, etype, desc in ships:
    stmts.append(make_cypher(code, name, etype, desc))

# Batch send (50 per request to avoid timeouts, but 49 fits in one)
batch_size = 50
total = 0
for i in range(0, len(stmts), batch_size):
    batch = stmts[i:i+batch_size]
    if run(batch):
        total += len(batch)
        print(f"   {total}/{len(stmts)}")

print(f"   Done: {total} entities imported")

# ── Step 3: Verify ──
print("\n3. Verification...")
verify_cypher = """
MATCH (n:Entity)
RETURN count(n) AS total,
       n.domain AS domain,
       count(DISTINCT n.type) AS types
"""
body = json.dumps({'statements': [{'statement': 'MATCH (n:Entity) RETURN count(n) AS total, collect(DISTINCT n.type) AS types'}]}, ensure_ascii=False).encode('utf-8')
req = urllib.request.Request(URL, data=body, headers={'Content-Type': 'application/json; charset=utf-8'})
req.add_header('Authorization', AUTH)
result = json.loads(urllib.request.urlopen(req).read())['results'][0]['data'][0]['row']
print(f"   Total nodes: {result[0]}")
print(f"   Types: {len(result[1])}")
for t in result[1]:
    if t:
        print(f"     - {t}")

print("\nDone!")
