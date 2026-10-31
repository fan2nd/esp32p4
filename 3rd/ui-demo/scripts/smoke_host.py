"""Local HTTP adapter smoke test. Pass the compiled host demo binary path."""
import json
import pathlib
import subprocess
import sys
import time
import urllib.request
root = pathlib.Path(__file__).resolve().parents[1]
binary = pathlib.Path(sys.argv[1]).resolve() if len(sys.argv)>1 else root/'target/debug/demo'
p = subprocess.Popen([str(binary)],stdout=subprocess.DEVNULL)
try:
    def get(path): return urllib.request.urlopen('http://127.0.0.1:7878'+path,timeout=3).read()
    for _ in range(30):
        try: get('/state'); break
        except OSError: time.sleep(.1)
    def state(): return json.loads(get('/state'))
    def event(kind, **fields): return get('/event?kind='+kind+''.join('&'+k+'='+str(v) for k,v in fields.items()))
    def click(x,y): event('down',x=x,y=y);return event('up',x=x,y=y)
    assert len(get('/frame'))==360*640*4
    assert b'canvas' in get('/')
    assert state()==dict(volume=35,mode=0,selected=None,scroll=0)
    event('down',x=32,y=170);event('move',x=327,y=170);event('up',x=327,y=170)
    assert state()['volume']==100
    click(200,240);click(200,368);assert state()['mode']==2 and state()['selected'] is None
    event('down',x=80,y=500);event('move',x=80,y=350);event('up',x=80,y=350)
    assert state()['scroll']==150 and state()['selected'] is None
    click(80,330);assert state()['selected']==2
    click(160,600);assert state()==dict(volume=35,mode=2,selected=None,scroll=150)
    event('keydown',key='next');event('keydown',key='right');assert state()['volume']==36
    event('cancel')
    print('PASS: HTTP/frame, volume, popup capture, drag/select, reset policy, keyboard')
finally:
    p.terminate();p.wait()
