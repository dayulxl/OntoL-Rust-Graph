import json, urllib.request, base64

AUTH = 'Basic ' + base64.b64encode(b'neo4j:12345678').decode()
URL = 'http://localhost:7474/db/neo4j/tx/commit'

def query(cypher):
    body = json.dumps({'statements': [{'statement': cypher}]}).encode('utf-8')
    req = urllib.request.Request(URL, data=body, headers={'Content-Type': 'application/json'})
    req.add_header('Authorization', AUTH)
    resp = json.loads(urllib.request.urlopen(req).read())
    for r in resp['results']:
        cols = r['columns']
        for d in r['data']:
            print(dict(zip(cols, d['row'])))

print('=== Total Entity count ===')
query('MATCH (n:Entity) RETURN count(n) AS total')

print('\n=== P-8A ===')
query("MATCH (n:Entity {code: 'P8A_001'}) RETURN n.name, n.code, n.`作战半径`, n.`最大起飞重量_kg_`, n.`机长`")

print('\n=== MK-54-1 ===')
query("MATCH (n:Entity {code: 'MK54_001'}) RETURN n.name, n.code, n.`射程`, n.`战斗部`, n.`速度`")

print('\n=== Types summary ===')
query("MATCH (n:Entity) RETURN n.type AS type, count(n) AS cnt ORDER BY cnt DESC")

print('\n=== CVN sample ===')
query("MATCH (n:Entity {code: 'CVN78'}) RETURN n.name, n.code, n.type, n.description")
