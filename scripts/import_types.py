"""导入分类层级关系 — (:Type)-[:subClassOf]->(:Type)"""
import json, urllib.request, base64

AUTH = 'Basic ' + base64.b64encode(b'neo4j:12345678').decode()
URL = 'http://localhost:7474/db/neo4j/tx/commit'

def run(stmts):
    body = json.dumps({'statements': [{'statement': s} for s in stmts]}, ensure_ascii=False).encode('utf-8')
    req = urllib.request.Request(URL, data=body, headers={'Content-Type': 'application/json; charset=utf-8'})
    req.add_header('Authorization', AUTH)
    resp = json.loads(urllib.request.urlopen(req).read())
    if resp.get('errors'):
        for e in resp['errors']:
            print(f'  ERROR: {e["message"]}')
        return False
    return True

pairs = [
    ['MH-60R海鹰反潜直升机', '直升机'],
    ['MQ-4C人鱼海神无人机', '无人机'],
    ['Mk_46_Mod_5_轻型反潜鱼雷', '鱼雷'],
    ['Mk_48_ADCAP重型鱼雷', '鱼雷'],
    ['Mk_54_轻型反潜鱼雷', '鱼雷'],
    ['P-8A海神巡逻机', '固定翼飞机'],
    ['主动声纳浮标', '声纳浮标'],
    ['供应级', '补给舰'],
    ['卫星通讯', '通讯'],
    ['回声旅行者（Echo_Voyager）', 'UUV'],
    ['固定翼飞机', '有人机'],
    ['声纳浮标', '传感器'],
    ['导弹', '武器'],
    ['尼米兹级', '航母'],
    ['巡洋舰', '水面舰'],
    ['弗吉尼亚级', '攻击型'],
    ['战斧巡航导弹', '导弹'],
    ['战略型', '潜艇'],
    ['提康德罗加级', '巡洋舰'],
    ['攻击型', '潜艇'],
    ['无人机', '飞机'],
    ['无线通讯', '通讯'],
    ['有人机', '飞机'],
    ['有线通讯', '通讯'],
    ['气象环境', '环境'],
    ['海底环境', '环境'],
    ['海洋测温浮标', '声纳浮标'],
    ['海洋环境', '环境'],
    ['电子战系统', '指挥系统'],
    ['直升机', '有人机'],
    ['磁异探测', '传感器'],
    ['福特级', '航母'],
]

stmts = []
for child, parent in pairs:
    cypher = f"""
MERGE (c:Type {{name: '{child}'}})
MERGE (p:Type {{name: '{parent}'}})
MERGE (c)-[:subClassOf]->(p)
"""
    stmts.append(cypher)

batch_size = 50
for i in range(0, len(stmts), batch_size):
    batch = stmts[i:i+batch_size]
    if run(batch):
        print(f'   {min(i+batch_size, len(stmts))}/{len(stmts)}')

# Verify
verify = [
    "MATCH (t:Type) RETURN count(t) AS type_nodes",
    "MATCH ()-[r:subClassOf]->() RETURN count(r) AS relationships",
    "MATCH (t:Type)<-[:subClassOf]-(c:Type) RETURN t.name AS parent, count(c) AS children ORDER BY children DESC LIMIT 10",
]
body = json.dumps({'statements': [{'statement': s} for s in verify]}, ensure_ascii=False).encode('utf-8')
req = urllib.request.Request(URL, data=body, headers={'Content-Type': 'application/json'})
req.add_header('Authorization', AUTH)
results = json.loads(urllib.request.urlopen(req).read())['results']
print(f"\n✅ Type nodes: {results[0]['data'][0]['row'][0]}")
print(f"✅ subClassOf: {results[1]['data'][0]['row'][0]}")
print(f"\nTop parents:")
for d in results[2]['data']:
    print(f"   {d['row'][0]} → {d['row'][1]} children")
