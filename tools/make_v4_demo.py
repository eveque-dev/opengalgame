"""Original three-chapter logic/debugger acceptance story; reuses bundled demo art."""
from pathlib import Path
import json,shutil
ROOT=Path(__file__).resolve().parents[1]
base=ROOT/'examples/first-light'
p=json.loads((base/'project.json').read_text(encoding='utf-8'))
dest=ROOT/'examples/harbor-signals';dest.mkdir(exist_ok=True)
shutil.copytree(base/'assets',dest/'assets',dirs_exist_ok=True)
p.update(format_version=2,id='opengalgame-harbor-signals-v4',title='灯塔来信',author='OpenGalgame · 0.4 验收故事',start='return',variables={'affinity':0,'clues':0,'signature':'旅人','promised':False},flags={'read_letter':False},tasks=[])
p.pop('save_compatibility',None)
first,second=p['characters'][:2]
p['characters']=[dict(first,id='qiao',name='乔安'),dict(second,id='lin',name='林澈'),dict(second,id='cen',name='岑舟'),dict(first,id='shen',name='沈音')]
def compare(key,op,value):return {'kind':'compare','variable':key,'op':op,'value':value}
def effect(key,value,add=False):return {'kind':'add','variable':key,'amount':value} if add else {'kind':'set','variable':key,'value':value}
def choice(text,target,when=None,effects=None):
    c={'text':text,'target':target,'condition':None,'set_flag':None}
    if when:c['when']=when
    if effects:c['effects']=effects
    return c
nodes=[]
def node(id,title,text,chapter,character=None,next=None,choices=None,ending=False):
    n={'id':id,'title':title,'text':text,'chapter':chapter,'kind':'ending' if ending else 'choice' if choices else 'dialogue','character':character,'background':p['backgrounds'][0]['id'],'next':next,'choices':choices or [],'position':[32+(len(nodes)%4)*285,32+(len(nodes)//4)*190]};nodes.append(n)
node('return','回到灯塔','离开三年后，你又在码头听见了自己的名字。乔安抱着一只旧信箱，像在等一个终于肯回信的人。','第一章 · 归来','qiao','meeting')
node('meeting','第一件小事','信箱的锁生锈了。林澈说，修好它，也许就能找到当年没有寄出的那封信。','第一章 · 归来',choices=[choice('留下帮乔安修锁','lock',effects=[effect('affinity',30,True),effect('clues',1,True),effect('promised',True),effect('signature','归乡人')]),choice('先听岑舟讲当年的事','lock',effects=[effect('clues',2,True)])])
node('lock','锁里的纸屑','岑舟记得每班离港的船，却一直没有说出最后一个乘客的名字。','第一章 · 归来','cen','archive')
node('archive','档案室的灯','沈音翻出值班簿：那天的灯塔并没有熄灭，只是有人换了守灯的班。','第一章 · 归来','shen','letter')
node('letter','拆开那封信','乔安把信递给你。这一次，她没有替你决定是否打开。','第二章 · 旧信',choices=[choice('读完信，把误会说清','reply',effects=[effect('read_letter',True),effect('affinity',20,True)]),choice('先把信收好，继续查值班簿','reply',effects=[effect('clues',1,True)])])
node('reply','迟到的解释','林澈承认自己替朋友保管了信，却不知道沉默也会变成一道门。','第二章 · 旧信','lin','witness')
node('witness','另一位守灯人','沈音说，证据能解释过去，却不能替任何人选择明天。','第二章 · 旧信','shen','departure')
node('departure','潮水转向','天快亮了。五条路都通向海边，只有一条会成为你的下一封信。','第三章 · 明天','cen','decision')
node('decision','你的答案','你准备把怎样的答案留在信箱里？隐藏选项取决于之前的好感、线索、承诺与署名。','第三章 · 明天',choices=[
    choice('和乔安一起留下守灯','ending_promise',{'kind':'all','items':[compare('affinity','ge',50),compare('read_letter','eq',True)]}),
    choice('把值班簿中的真相公开','ending_truth',{'kind':'any','items':[compare('clues','ge',2),compare('promised','eq',True)]}),
    choice('搭第一班船，去新的城市','ending_travel'),
    choice('保留没有拆开的信，约定下次见面','ending_quiet',{'kind':'all','items':[compare('affinity','ge',30),compare('read_letter','eq',False)]}),
    choice('以旅人的名字寄出一封新信','ending_letter',compare('signature','eq','旅人'))])
for id,title,text,cast in [('ending_promise','结局：共同守灯','你们重新排了一张值班表。约定终于从一句话，变成了每天都会亮起的灯。','qiao'),('ending_truth','结局：有名的真相','你和岑舟把每个日期都核对了一遍。港口公告栏上，终于没有被省略的名字。','cen'),('ending_travel','结局：海的另一边','林澈替你提着行李，走到船边才松手。分别也可以是一种认真回答。','lin'),('ending_quiet','结局：未拆封的明天','乔安没有催你拆信。你们把下次见面的日期写在信封背面。','qiao'),('ending_letter','结局：第一封新信','沈音借给你钢笔。你写下旅人的名字，决定从这一页重新认识这座港口。','shen')]:node(id,title,text,'第三章 · 明天',cast,ending=True)
p['nodes']=nodes
(dest/'project.json').write_text(json.dumps(p,ensure_ascii=False,indent=2),encoding='utf-8')
print(f'Created {len(nodes)} nodes / 5 endings')
