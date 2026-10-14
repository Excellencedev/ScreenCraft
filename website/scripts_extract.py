import re,glob,json,html
files=['index.html','features.html']+sorted(glob.glob('docs/*.html'))+['src/partials/docs-nav.html']
tag=re.compile(r'<(h[1-5]|p|li|dt|dd|th|td|summary)(\s[^>]*)?>(.*?)</\1>',re.S)
navlink=re.compile(r'<a href="[^"]*">([^<]+)</a>')
btn=re.compile(r'<a class="btn[^"]*"[^>]*>([^<]+)</a>')
seen={}; order=[]
def norm(s): return re.sub(r'\s+',' ',s).strip()
def add(s):
    s=norm(s)
    if not re.search(r'[A-Za-z]{3,}',re.sub(r'<[^>]+>','',s)): return
    if re.search(r'<(p|div|ul|ol|li|table|h\d|details)\b',s): return
    if s not in seen: seen[s]=len(order); order.append(s)
for f in files:
    src=open(f).read()
    src=re.sub(r'<!--.*?-->','',src,flags=re.S)
    src=re.sub(r'<pre>.*?</pre>','',src,flags=re.S)
    def walk(text):
        for m in tag.finditer(text):
            attrs=m.group(2) or ''
            inner=m.group(3)
            if re.search(r'<(h[1-5]|p|div|ul|ol|li|table|details)\b',inner):
                walk(inner); continue
            if 'data-i18n' in attrs: continue
            add(inner)
    walk(src)
    if f.endswith('docs-nav.html'):
        for m in navlink.finditer(src): add(m.group(1))
    for m in btn.finditer(src):
        if 'data-i18n' not in m.group(0): add(m.group(1))
json.dump(order,open('/tmp/en_strings.json','w'),ensure_ascii=False,indent=0)
print(len(order))
for i,s in enumerate(order): print(f"{i}\t{s}")
