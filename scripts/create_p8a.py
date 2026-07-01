import json, urllib.request, base64

AUTH = 'Basic ' + base64.b64encode(b'neo4j:12345678').decode()
URL = 'http://localhost:7474/db/neo4j/tx/commit'

BASE = """n.id = randomUUID(), n.graph_id = randomUUID(),
  n.domain = 'Anti-SubmarineWarfare', n.leven = 1,
  n.update_time = datetime(), n.create_time = datetime(),
  n.confidence = 1.0, n.status = '有效', n.version = '1.0',
  n.cope_version = '1.0', n.source = 'OWL_Import', n.owner = 'Admin',
  n.Space_abs = [23.1291, 12.8, -30.0, 0.0],
  n.command_side = 1,
  n.precondition = '', n.effect = '', n.cost = '',
  n.duration = 100, n.priority = 0, n.composedOf = ''"""

EXTRA = """, n.`作战半径` = 2222, n.`实用升限` = 12500, n.`巡航速度` = 815,
  n.`最大航程` = 8300, n.`最大起飞重量_kg_` = 85820, n.`最大速度` = 907,
  n.`机长` = 39.47, n.`机高` = 12.83, n.`续航时间` = 10, n.`翼展` = 37.64"""

p8as = [
    ("P8A_001", "P-8A海神巡逻机-1"),
    ("P8A_002", "P-8A海神巡逻机-2"),
    ("P8A_003", "P-8A海神巡逻机-3"),
    ("P8A_004", "P-8A海神巡逻机-4"),
]

stmts = []
for code, name in p8as:
    cypher = f"""MERGE (n:Entity {{code: '{code}'}})
ON CREATE SET n.name = '{name}', {BASE}
SET n.type = 'P-8A海神巡逻机', n.description = '{name}'{EXTRA}"""
    stmts.append(cypher)

body = json.dumps({'statements': [{'statement': s} for s in stmts]}, ensure_ascii=False).encode('utf-8')
req = urllib.request.Request(URL, data=body, headers={'Content-Type': 'application/json; charset=utf-8'})
req.add_header('Authorization', AUTH)
result = json.loads(urllib.request.urlopen(req).read())
if result.get('errors'):
    for e in result['errors']:
        print(f"ERROR: {e['message']}")
else:
    print(f"Done: {len(p8as)} P-8A instances")

# Verify
verify = "MATCH (n:Entity) WHERE n.type = 'P-8A海神巡逻机' RETURN n.code, n.name, n.`作战半径`, n.`续航时间` ORDER BY n.code"
body2 = json.dumps({'statements': [{'statement': verify}]}, ensure_ascii=False).encode('utf-8')
req2 = urllib.request.Request(URL, data=body2, headers={'Content-Type': 'application/json; charset=utf-8'})
req2.add_header('Authorization', AUTH)
for d in json.loads(urllib.request.urlopen(req2).read())['results'][0]['data']:
    print(f"  {d['row']}")
