"""Build the original 0.5 direction/voice acceptance story using bundled demo art."""
from pathlib import Path
import array, json, math, random, shutil, wave

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "examples/harbor-signals"
DEST = ROOT / "examples/tidal-stage"
(DEST / "assets").mkdir(parents=True, exist_ok=True)
p = json.loads((SOURCE / "project.json").read_text(encoding="utf-8"))
for name in ["bay.webp", "city.webp", "qiao.webp", "lin.webp"]:
    shutil.copy2(SOURCE / "assets" / name, DEST / "assets" / name)
p.update(format_version=3, id="opengalgame-tidal-stage-v5", title="潮汐剧场", author="OpenGalgame · 演出与配音样片", start="opening", flags={}, variables={"affinity": 0, "clues": 0, "promised": False, "visits": 0}, tasks=[])
p.pop("save_compatibility", None)
for c in p["characters"]:
    c["expressions"] = {"微笑" if c["id"] in ["qiao", "shen"] else "认真": c["portrait"]}
p.setdefault("presentation", {}).update(bgm="music_harbor", font_size=23, transition_seconds=0.3, typewriter_cps=24)
p["production"] = {"theme": {"name": "潮汐 · 夜航", "panel": [17, 30, 43], "text": [237, 241, 240], "name_color": [166, 219, 209], "choice": [43, 67, 79], "choice_text": [240, 245, 243], "menu": [14, 27, 38], "panel_height": 0.43, "name_box": True, "menu_position": "center", "save_columns": 2}, "cases": []}
rate = 22050
rng = random.Random(51)
def wav(name, seconds, generator):
    pcm = array.array("h", (int(max(-1, min(1, generator(i / rate))) * 32767) for i in range(int(rate * seconds))))
    with wave.open(str(DEST / "assets" / name), "wb") as f:
        f.setnchannels(1); f.setsampwidth(2); f.setframerate(rate); f.writeframes(pcm.tobytes())
wav("harbor.wav", 8, lambda t: sum(math.sin(2 * math.pi * hz * t) for hz in [130.81, 164.81, 196]) * 0.035 * (0.7 + 0.3 * math.sin(math.pi * t / 4)))
wav("waves.wav", 8, lambda t: (rng.random() * 2 - 1) * 0.07 * (0.5 + 0.5 * math.sin(math.pi * t / 2) ** 2))
wav("bell.wav", 0.8, lambda t: math.sin(2 * math.pi * 660 * t) * math.exp(-6 * t) * 0.18)
p["audio"] = [dict(id="music_harbor", name="港湾和弦", file="assets/harbor.wav"), dict(id="amb_waves", name="海边环境声", file="assets/waves.wav"), dict(id="fx_bell", name="钟声", file="assets/bell.wav")]
def add(key, amount): return dict(kind="add", variable=key, amount=amount)
def set_value(key, value): return dict(kind="set", variable=key, value=value)
def compare(key, op, value): return dict(kind="compare", variable=key, op=op, value=value)
def show(character, side="left", seconds=0.4): return dict(kind="show", character=character, expression=None, side=side, scale=1, seconds=seconds)
def move(character, side): return dict(kind="move", character=character, side=side, scale=1, seconds=0.6)
def option(text, target, when=None, effects=None):
    o = dict(text=text, target=target)
    if when: o["when"] = when
    if effects: o["effects"] = effects
    return o
nodes = []
def node(id, title, text, speaker=None, next=None, kind="dialogue", actions=None, enter=None, exit=None, choices=None, chapter="第一幕 · 回声"):
    n = dict(id=id, title=title, kind=kind, text=text, character=speaker, background="bay", next=next, actors=[], position=[len(nodes)%3*270, len(nodes)//3*210], chapter=chapter)
    n["direction"] = dict(actions=actions or [], enter=enter or [], exit=exit or [], voice="voice_"+id)
    if choices: n["choices"] = choices
    nodes.append(n)
    p["audio"].append(dict(id="voice_"+id, name="配音 · "+title, file="assets/"+id+".wav"))
    return n
opening = node("opening", "最后一班潮汐", "剧场关门前，海边的钟又响了一次。你带着那封迟到的信，回到了排练开始的地方。", next="entrance", actions=[dict(kind="wait", seconds=0.35)], enter=[add("visits",1),add("clues",1)])
opening["direction"]["audio"] = [dict(channel="ambience", audio="amb_waves", volume=0.45, looped=True, fade=1)]
node("entrance", "两个人的开场", "你终于来了。林澈说今晚不排结局，我们先把那扇老窗修好，让海风自己走进来。", "qiao", "meeting", actions=[show("qiao"),show("lin","right"),move("qiao","center"),dict(kind="expression",character="qiao",expression="微笑"),dict(kind="wait",seconds=0.25)])
node("meeting", "先听谁的声音", "一边是等待修好的窗，一边是写着旧日期的值班簿。你决定先做哪一件事？",kind="choice",choices=[option("留下来帮林澈修窗","repair",effects=[add("affinity",40)]),option("去档案室查清那天的事","archive",effects=[add("clues",2)])])
repair=node("repair", "窗开了", "木框终于不再吱呀作响。林澈把工具放下，说有些事情只要一起做过，就不会再害怕。", "lin", "reply",actions=[move("lin","center"),move("qiao","left")],exit=[add("affinity",10)],chapter="第二幕 · 来信")
repair["direction"]["audio"]=[dict(channel="effects",audio="fx_bell",volume=0.6,looped=False,fade=0)]
node("archive", "另一位证人", "沈音把值班簿递给你。岑舟一直记得那盏灯，只是从来没有找到一个愿意听完的人。", "shen", "reply",actions=[dict(kind="hide",character="qiao",seconds=0.3),show("cen"),show("shen","right")],enter=[add("clues",1)],chapter="第二幕 · 来信")
node("reply", "信封上的潮痕", "原来那封信没有被忘记。它只是等着收信的人，愿意再把故事听一遍。", "qiao", "letter",actions=[show("qiao","center")],chapter="第二幕 · 来信")
node("letter", "拆开还是保留", "信纸很轻，可你知道，这一次选择之后，总得有人认真回答。",kind="choice",choices=[option("现在拆开，把话说清楚","dawn",effects=[add("affinity",20)]),option("留到明天，约定一起再来","dawn",effects=[set_value("promised",True),add("clues",1)])],chapter="第二幕 · 来信")
node("dawn", "天亮以前", "潮水已经转向。你可以留下，也可以远行，但这一次，没有谁需要替别人决定。", "cen", "decision",actions=[dict(kind="hide",character="shen",seconds=0.3),move("cen","left"),move("qiao","right")],chapter="第三幕 · 明天")
node("decision", "写下你的结局", "剧场里的人都停下了手里的事，等你说出最后一句对白。",kind="choice",choices=[option("留下来，一起把剧场开下去","ending_home",compare("affinity","ge",60)),option("公开值班簿里的真相","ending_truth",compare("clues","ge",3)),option("守住约定，明天再见","ending_promise",compare("promised","eq",True)),option("带着信，去海的另一边","ending_sea")],chapter="第三幕 · 明天")
for id,title,text,speaker in [("ending_home","结局 · 灯火常亮","你们重新排好了值班表。海风穿过修好的窗，而灯火会一直留给晚归的人。","qiao"),("ending_truth","结局 · 有名字的回声","值班簿终于被读到了最后一页。那些沉默的日子，都有了可以被记住的名字。","shen"),("ending_promise","结局 · 明天的座位","信还没有拆开，座位却已经留好。原来约定的意思，是有人会认真等你再来。","lin"),("ending_sea","结局 · 下一座港口","你在天亮时离港。信纸上多写了一行地址，下一次告别，也可以是一封新信的开头。","cen")]:
    ending=node(id,title,text,speaker,kind="ending",chapter="终幕",actions=[dict(kind="wait",seconds=0.3)])
    ending["direction"]["audio"]=[dict(channel="ambience",audio=None,volume=1,looped=True,fade=1.2)]
p["nodes"] = nodes
(DEST / "project.json").write_text(json.dumps(p,ensure_ascii=False,indent=2),encoding="utf-8")
print(f"Created {len(nodes)} nodes, four endings and original synthesized ambience")
