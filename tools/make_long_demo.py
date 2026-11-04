from pathlib import Path
import json,math,shutil,wave,struct

ROOT=Path(__file__).resolve().parents[1]
DEST=ROOT/'examples/long-form';ASSETS=DEST/'assets';ASSETS.mkdir(parents=True,exist_ok=True)
for name in ['bay.webp','city.webp','lin.webp','qiao.webp']:
    shutil.copy2(ROOT/'examples/first-light/assets'/name,ASSETS/name)
assert (ASSETS/'icon.png').is_file(), 'Keep the bundled icon.png or supply your own before rebuilding this example.'

def sound(name,seconds,music=False):
    rate=22050;values=[]
    chords=[(220,277.18,329.63),(174.61,220,261.63),(196,246.94,293.66),(164.81,196,246.94)]
    for i in range(int(seconds*rate)):
        t=i/rate
        if music:
            phase=t%2;notes=chords[int(t//2)%4];envelope=(1-math.exp(-phase*12))*math.exp(-phase*1.7)
            value=sum(math.sin(2*math.pi*f*t)*0.025 for f in notes)*envelope
            value*=min(1,t/0.05,(seconds-t)/0.05)
        else:value=math.sin(2*math.pi*660*t)*0.12*math.exp(-t*9)*min(1,t/0.01)
        values.append(struct.pack('<h',int(max(-1,min(1,value))*32767)))
    with wave.open(str(ASSETS/name),'wb')as w:w.setnchannels(1);w.setsampwidth(2);w.setframerate(rate);w.writeframes(b''.join(values))
sound('quiet-tide.wav',16,True);sound('paper-bell.wav',0.5)

p={'format_version':1,'id':'lumen-long-form-acceptance-v2','title':'雨停之前：三封来信','author':'映叙工坊 · 三路线验收示例','start':'c01',
   'characters':[{'id':'qiao','name':'乔安','color':[240,193,170],'portrait':'assets/qiao.webp','expressions':{'平静':'assets/qiao.webp'},'tags':'旧友，海湾'},
                 {'id':'lin','name':'林澈','color':[154,202,212],'portrait':'assets/lin.webp','expressions':{'平静':'assets/lin.webp'},'tags':'归来者'}],
   'backgrounds':[{'id':'bay','name':'雨后的海湾','file':'assets/bay.webp'},{'id':'city','name':'城里的站台','file':'assets/city.webp'}],
   'audio':[{'id':'tide','name':'安静潮汐 · 原创合成和弦','file':'assets/quiet-tide.wav'},{'id':'bell','name':'信纸轻响 · 原创合成提示音','file':'assets/paper-bell.wav'}],
   'presentation':{'cover':'assets/bay.webp','icon':'assets/icon.png','accent':[169,156,241],'font_size':22,'typewriter_cps':28,'transition_seconds':0.35,'bgm':'tide'},
   'flags':{'kept_letter':False,'asked_truth':False},'nodes':[]}

def node(id,title,text,chapter,character='qiao',kind='dialogue',next=None,bg='bay',choices=None):
    n={'id':id,'title':title,'kind':kind,'chapter':chapter,'text':text,'character':character,'background':bg,'next':next,'choices':choices or [],'position':[0,0],
       'portrait_side':'right','portrait_scale':1.0,'expression':'平静' if character else None,'actors':[],'transition':'fade' if id.endswith('01')else'none','bgm':None,'sound':'bell'if kind=='choice'else None}
    if id in ['c02','c08','a03','d05']:
        n['actors']=[{'character':'lin','expression':'平静','side':'left','scale':0.9}]
        if character=='lin':n['character']='qiao'
    if id.endswith('04'):n['transition']='slide'
    p['nodes'].append(n);return n

common=[
('最后一班船','我以为你不会再回来。码头每天都有人上下船，可我总会在最后一声汽笛里，再认一次你的脚步。'),
('潮声里的旧名字','这里和从前一样，连栏杆缺掉的那一角都没有修好。我记得我们曾在这里说，要把每一个晴天都记下来。'),
('没有寄出的信','这三封信原本要寄去三个地方：旧屋、车站，还有报馆。后来我才知道，同一段往事，也可以有三种不同的讲法。'),
('迟到的解释','我离开的那天，雨比今天还大。不是所有沉默都意味着忘记，有些时候，一个人只是还没找到能让对方听明白的话。'),
('折痕','你看，这里折过两次。我一直没有打开最后一页。我想，若是你回来，应该由你决定这封信要留在哪里。'),
('留下这封信吗','在离开码头之前，先决定是否收下这封旧信。它会成为这次归来里一件可以带走、也可以留下的东西。'),
('雨伞之下','雨伞只够两个人并肩走，谁也无法装作走在很远的地方。旧友之间有时需要的，恰恰是这样一点无法逃开的距离。'),
('灯重新亮了','旧屋的灯还亮着。有人说，只要灯还亮着，回来就不算太迟。但灯也会熄灭，我们需要在那之前把话说清。'),
('第三封信','寄往报馆的信里提到了一件没有登报的小事。它改变了几个人的去向，却从未出现在那些漂亮的告别词里。'),
('是否追问','你可以问清报馆那封信的来由，也可以先照顾眼前的人。选择追问会打开第三条路线；沉默仍能通向另外两个结局。'),
('三种明天','有人选择留下，有人选择远行，也有人想在出发以前找到真相。我们不必替彼此决定，但必须为自己的选择负责。'),
('决定去向','雨正在变小。旧屋的门、最后一班列车，还有报馆清晨的第一盏灯，都在等待一个回答。')]
for i,(title,text)in enumerate(common,1):node(f'c{i:02}',title,text,'归来',character='lin'if i in[2,4,7,11]else'qiao',next=f'c{i+1:02}'if i<12 else None)
p['nodes'][5].update(kind='choice',next=None,choices=[{'text':'收下旧信，认真保管。','target':'c07','set_flag':{'flag':'kept_letter','value':True}},{'text':'让信留在旧屋。','target':'c07','set_flag':{'flag':'kept_letter','value':False}}],sound='bell')
p['nodes'][9].update(kind='choice',next=None,choices=[{'text':'问清第三封信的真相。','target':'c11','set_flag':{'flag':'asked_truth','value':True}},{'text':'此刻先陪她走完这段路。','target':'c11','set_flag':{'flag':'asked_truth','value':False}}],sound='bell')
p['nodes'][11].update(kind='choice',next=None,choices=[{'text':'留下来，把旧屋的窗重新打开。','target':'a01'},{'text':'去车站，好好告别后出发。','target':'b01'},{'text':'去报馆，让被省略的事情留下记录。','target':'d01','condition':{'flag':'asked_truth','value':True}}],sound='bell')
branches={
'a':('留下',[
('重新开门','钥匙还放在花盆底下。那是一种笨拙却固执的信任，像这间屋子始终没有换掉的窗帘。'),
('擦去灰尘','他们花了一整个下午擦窗，水盆换了三次。许多原本以为非说不可的话，就在递抹布的时候慢慢说完了。'),
('晚餐','两份简单的晚餐，一盏不太亮的灯。生活不会因为一次归来就变得容易，但至少有人愿意一起商量明天。'),
('修好的栏杆','码头的工人说，那一角栏杆其实很好修，只是一直没人报修。有些缺口也是如此，要先承认它还在。'),
('把话说完','我不想把留下当作补偿。你可以改变主意，也可以有自己的工作和朋友。我希望我们都是愿意留在这里的人。'),
('信的位置','旧信放进了抽屉，也有一些话被重新写在纸上。过去得到一个位置，就不必再占据所有的明天。'),
('约定的尺度','他们没有承诺永远，只约好在离开之前提前说，在难过的时候不让沉默替自己回答。'),
('第一个晴天','清晨的阳光照过洗净的窗。海面没有发生什么奇迹，屋里的人却终于能把早餐吃完。'),
('还会下雨','天气预报说，周末还会有雨。他们把两把雨伞放在门边，谁也没有因此觉得这个晴天不值得珍惜。'),
('结局：晴天的约定','旧屋没有变成永不散场的舞会，只成为可以认真生活的地方。他们学着在同一个屋檐下保持各自的自由，也学着把想说的话说在来得及的时候。\n\n留下，不是重演过去，而是一起安排一个新的明天。')]),
'b':('远行',[
('买票','售票员问，是单程还是往返。他想了想，买下了一张单程票。告别的诚意，不需要用一张尚未决定日期的回程票证明。'),
('站台','站台上的人不多，行李碰撞木地板的声音却格外清楚。他们终于有时间把那一次匆忙的离开重新说一遍。'),
('把误会归还','原来那些被当作冷漠的片段，也包含过犹豫和害怕。知道这些不能让一切恢复原样，却能让人不再一直责怪自己。'),
('各自的路','你不需要为了证明我重要而放弃出发。你曾在这里认真生活过，这件事不会被一张车票擦掉。'),
('等待发车','他们坐在长椅的两端，分享一袋已经有点凉的面包。没有谁再试图把这十分钟延长成一生。'),
('新的地址','林澈把新地址写在纸上。不是保证每一封信都会得到及时回复，而是让彼此在愿意的时候，仍能找到一个可以寄信的地方。'),
('最后一次挥手','列车员关上车门的时候，他们没有追着说更多的话。那些重要的话已经说完，剩下的就交给各自的日子。'),
('窗外','海湾从窗外慢慢退去，城里的灯亮了起来。他发现自己可以想念一个地方，同时也愿意看看尚未抵达的生活。'),
('第一封新信','抵达以后，他先写了一封很短的信：我平安到了。窗外有一棵树，我还不知道它的名字。'),
('结局：开往明天','列车驶出了雨幕，旧屋的灯也按时熄灭。有人去往新的城市，有人留在熟悉的海湾；他们不再把不同的去向当成辜负。\n\n有些告别让故事结束，有些告别让每个人重新成为自己。')]),
'd':('真相',[
('报馆的清晨','报馆的第一盏灯比太阳亮得更早。编辑听完他们的来意，把那封几乎被归档的来信重新放到了桌上。'),
('省略的名字','原来的报道写了许多体面的人，却省略了一个普通工人的名字。那个人的证词，正是他们所有误会的起点。'),
('核对日期','他们先核对日期，再寻找当时的值班记录。真相并不总是一段慷慨激昂的独白，更多时候是一页页需要仔细比对的纸。'),
('第二位证人','第二位证人住在城郊。他记得那天的雨，也记得有人在所有人离开以后，独自把散落的信件收了起来。'),
('互相确认','这一次，乔安和林澈没有替对方省略难以接受的部分。他们把每一条新消息摊开，一起确认哪里还需要证据。'),
('更正','报馆同意刊登更正。字不大，位置也不显眼，但那些被写错和被省略的名字，终于得到了应有的位置。'),
('迟到的回信','他们把更正寄给仍在等待解释的人。有些回信来得很慢，有些人已经换了住址，但这并不意味着寄出的动作没有意义。'),
('雨中的采访','年轻记者问，这一切值得吗。乔安说，至少下一次有人翻到那一年时，不会只看见一种声音。'),
('记录之外','事情告一段落以后，他们还是要回到自己的生活。真相没有自动替他们决定留下或出发，却让决定终于有了完整的依据。'),
('结局：灯下有名','更正和原来的报道被放进同一个档案夹。许多年以后，有人翻开它，发现那一年发生的事情，比最初流传的故事多了几个人的名字。\n\n他们没有改写所有人的过去，却让后来的人有机会把故事看得更完整。')])}
for prefix,(chapter,rows)in branches.items():
    for i,(title,text)in enumerate(rows,1):
        node(f'{prefix}{i:02}',title,text,chapter,character=None if i==10 else('lin'if i%2==0 else'qiao'),kind='ending'if i==10 else'dialogue',next=f'{prefix}{i+1:02}'if i<10 else None,bg='bay'if prefix=='a'else'city')
for index,n in enumerate(p['nodes']):n['position']=[32+(index%6)*285,32+(index//6)*180]
assert len(p['nodes'])==42
(DEST/'project.json').write_text(json.dumps(p,ensure_ascii=False,indent=2),encoding='utf-8')
(DEST/'ASSETS.md').write_text('图片复用自工作区既有生成素材。quiet-tide.wav 与 paper-bell.wav 由本脚本的正弦波、和弦和包络算法原创合成，无录音或外部曲库。表情组中的“平静”引用默认立绘，可在编辑器中替换成自己的表情图片。\n',encoding='utf-8')
print('Created 42-node / 3-ending acceptance story with original synthesized audio.')
