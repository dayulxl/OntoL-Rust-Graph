import json, urllib.request, base64

AUTH = 'Basic ' + base64.b64encode(b'neo4j:12345678').decode()
URL = 'http://localhost:7474/db/neo4j/tx/commit'

def run(statements):
    body = json.dumps({'statements': [{'statement': s} for s in statements]}).encode('utf-8')
    req = urllib.request.Request(URL, data=body, headers={'Content-Type': 'application/json'})
    req.add_header('Authorization', AUTH)
    resp = json.loads(urllib.request.urlopen(req).read())
    if resp.get('errors'):
        for e in resp['errors']:
            print('ERROR:', e['message'])
        return False
    print(f'OK: {len(statements)} statements')
    return True

# P-8A properties
p8a = """MATCH (n:Entity {code: 'P8A_001'})
SET n.`作战半径` = 2222, n.`实用升限` = 12500, n.`巡航速度` = 815,
    n.`最大航程` = 8300, n.`最大起飞重量_kg_` = 85820, n.`最大速度` = 907,
    n.`机长` = 39.47, n.`机高` = 12.83, n.`续航时间` = 10, n.`翼展` = 37.64"""

# MK-54 properties
mk54 = []
for i in range(1, 9):
    code = f"MK54_{i:03d}"
    mk54.append(f"MATCH (n:Entity {{code: '{code}'}}) SET n.`射程` = 10, n.`战斗部` = 44.5, n.`深度` = 450, n.`速度` = 40")

run([p8a])
run(mk54)
print("Done!")
