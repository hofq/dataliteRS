use esp_idf_svc::http::server::EspHttpServer;

const HTML: &str = r#"<!DOCTYPE html>
<html><head><meta name="viewport" content="width=device-width,initial-scale=1,maximum-scale=1">
<title>Display</title>
<style>
*{margin:0;padding:0;box-sizing:border-box}
body{font:13px/1.4 monospace;background:#111;color:#0f0;padding:8px;user-select:none;-webkit-user-select:none}
.r{display:flex;gap:4px;margin:2px 0;align-items:center}
.n{width:22px;text-align:right;color:#666;flex-shrink:0;font-size:11px}
.g{display:flex;gap:2px;flex-wrap:nowrap;overflow:hidden;flex:1;max-width:max-content}
.g input{font:15px monospace;width:1.6ch;min-width:0;max-width:1.6ch;flex:0 0 1.6ch;background:#1a1a1a;color:#0f0;border:1px solid #222;padding:2px 0;text-align:center;caret-color:#0f0}
@media(max-width:700px){.g{max-width:100%}.g input{flex:1 1 0;width:auto;max-width:none}}
.g input:focus{border-color:#0f0;background:#222}
.g input.e{background:#111}
.g input.sel{border-color:#0f0!important;background:#003300!important}
.g input.b{color:#ff0;border-color:#553300}
.g input.b.e{background:#1a1500}
.tb{background:#0a0a0a;border:1px solid #222;padding:6px 8px;margin-bottom:4px;display:flex;gap:6px;align-items:center;flex-wrap:wrap;border-radius:4px}
.tb .sep{width:1px;height:20px;background:#333;flex-shrink:0}
.tb label{color:#666;font-size:11px;display:flex;align-items:center;gap:3px;white-space:nowrap}
.tb label span{color:#0f0;min-width:18px;text-align:center}
.tb select,.tb input[type=text]{font:12px monospace;background:#000;color:#0f0;border:1px solid #333;padding:2px 4px}
.tb input[type=range]{width:50px;accent-color:#0f0}
.tb input[type=checkbox]{accent-color:#0f0}
button{font:12px monospace;background:#0f0;color:#000;border:0;padding:4px 12px;cursor:pointer;border-radius:2px}
button.off{background:#333;color:#666}
button.sm{padding:4px 8px}
button.rst{background:#f00;color:#fff}
button.warn{background:#f80;color:#000}
#status{color:#888;font-size:11px}
.cfg{background:#1a1a1a;border:1px solid #333;padding:12px;margin:8px 0;display:none;border-radius:4px}
.cfg label{display:block;margin:4px 0;color:#888}
.cfg input[type=text],.cfg input[type=number],.cfg input[type=password]{font:13px monospace;background:#000;color:#0f0;border:1px solid #333;padding:3px;width:180px}
.cfg h3{color:#0f0;margin:8px 0 4px;font-size:13px}
.cfg .row{display:flex;gap:16px;flex-wrap:wrap}
.cfg .col{flex:1;min-width:180px}
</style></head><body>
<div class="tb">
<label>Page <select id="pg"></select></label>
<div class="sep"></div>
<label>Brightness <input type="range" id="bright" min="1" max="17"> <span id="brightval"></span></label>
<label>Duration <input type="text" id="time" style="width:36px">s</label>
<div class="sep"></div>
<label><input type="checkbox" id="scroll"> Scroll</label>
<label><input type="checkbox" id="fade"> Fade</label>
<label>Speed <input type="range" id="mspeed" min="1" max="20" style="width:50px"> <span id="msval"></span></label>
<div class="sep"></div>
<label><input type="checkbox" id="bold"> Bold</label>
<div class="sep"></div>
<button id="capbtn" class="off sm" onclick="toggleCap()">A/a</button>
<button id="blinkbtn" class="off sm" onclick="toggleBlink()">Blink</button>
<select id="blinkspd" style="display:none" title="Blink speed">
<option value="1">Slow</option><option value="2" selected>Med</option>
<option value="3">Fast</option><option value="4">Rapid</option>
</select>
<div class="sep"></div>
<button onclick="send()">Send</button>
<button class="sm" onclick="toggleCfg()">Cfg</button>
<span id="status"></span>
</div>
<div id="grid"></div>
<div style="margin-top:8px;display:flex;gap:8px">
<button class="rst" onclick="reset()">Reset All</button>
</div>
<div class="cfg" id="cfgpanel">
<div class="row">
<div class="col">
<h3>WiFi Client</h3>
<label>Network<br><input type="text" id="cfg_wifi_ssid"></label>
<label>Password<br><input type="password" id="cfg_wifi_pass"></label>
<label>Timeout (s)<br><input type="number" id="cfg_connect_timeout" min="5" max="120"></label>
</div>
<div class="col">
<h3>Hotspot</h3>
<label>Name<br><input type="text" id="cfg_ap_ssid"></label>
<label>Password<br><input type="password" id="cfg_ap_pass"></label>
</div>
<div class="col">
<h3>Display Hardware</h3>
<label>Controllers<br><input type="number" id="cfg_controllers" min="1" max="32"></label>
<label>Rows / Controller<br><input type="number" id="cfg_lines_per_ctrl" min="1" max="8"></label>
<label>Columns<br><input type="number" id="cfg_chars_per_line" min="1" max="64"></label>
<label>Visible Rows<br><input type="number" id="cfg_visible_lines" min="1" max="32"></label>
<label>Pages<br><input type="number" id="cfg_max_pages" min="1" max="8"></label>
</div>
</div>
<div style="margin-top:8px;display:flex;gap:8px">
<button onclick="saveCfg()">Save Settings</button>
<button class="warn" onclick="reboot()">Reboot</button>
</div>
</div>
<script>
var S,P=1,C,R,CAP=false,BL=[],SEL={on:false,r0:0,c0:0,r1:0,c1:0},TOUCH={tid:null,timer:null,sel:false};

// brightness display
document.getElementById('bright').oninput=function(){document.getElementById('brightval').textContent=this.value};
document.getElementById('mspeed').oninput=function(){document.getElementById('msval').textContent=this.value};

function toggleCap(){
CAP=!CAP;
document.getElementById('capbtn').className=CAP?'sm':'off sm';
document.querySelectorAll('.g input').forEach(function(i){i.autocapitalize=CAP?'characters':'off'});
}

function toggleCfg(){
var p=document.getElementById('cfgpanel');
if(p.style.display==='block'){p.style.display='none';return;}
p.style.display='block';
api('GET','settings').then(function(d){
['wifi_ssid','wifi_pass','connect_timeout','ap_ssid','ap_pass',
'controllers','lines_per_ctrl','chars_per_line','visible_lines','max_pages'].forEach(function(k){
var el=document.getElementById('cfg_'+k);if(el)el.value=d[k];
});
});
}
function saveCfg(){
var d={};
['wifi_ssid','wifi_pass','connect_timeout','ap_ssid','ap_pass',
'controllers','lines_per_ctrl','chars_per_line','visible_lines','max_pages'].forEach(function(k){
var el=document.getElementById('cfg_'+k);
d[k]=el.type==='number'?+el.value:el.value;
});
api('POST','settings',d).then(function(r){
if(r.reboot)alert('Settings saved. Reboot to apply.');
});
}
function reboot(){
if(!confirm('Reboot device?'))return;
api('POST','reboot').then(function(){document.getElementById('status').textContent='rebooting...';});
}

function hasSel(){return SEL.on&&!(SEL.r0===SEL.r1&&SEL.c0===SEL.c1);}

function toggleBlink(){
if(!hasSel())return;
var rn=Math.min(SEL.r0,SEL.r1),rx=Math.max(SEL.r0,SEL.r1);
var cn=Math.min(SEL.c0,SEL.c1),cx=Math.max(SEL.c0,SEL.c1);
var allB=true;
for(var r=rn;r<=rx;r++)for(var c=cn;c<=cx;c++)if(!BL[r][c])allB=false;
for(var r=rn;r<=rx;r++)for(var c=cn;c<=cx;c++)BL[r][c]=!allB;
clearSel();updateClasses();
}

function clearSel(){
SEL.on=false;
document.querySelectorAll('.g input.sel').forEach(function(i){i.classList.remove('sel')});
var bb=document.getElementById('blinkbtn');bb.className='off sm';
document.getElementById('blinkspd').style.display='none';
}

function updateClasses(){
for(var r=0;r<R;r++){
var g=document.getElementById('g'+(r+1));if(!g)continue;
for(var c=0;c<C;c++){
var inp=g.children[c],cls=[];
if(!inp.value)cls.push('e');
if(BL[r]&&BL[r][c])cls.push('b');
inp.className=cls.join(' ');
}}
}

function applySel(){
if(!SEL.on)return;
document.querySelectorAll('.g input.sel').forEach(function(i){i.classList.remove('sel')});
var rn=Math.min(SEL.r0,SEL.r1),rx=Math.max(SEL.r0,SEL.r1);
var cn=Math.min(SEL.c0,SEL.c1),cx=Math.max(SEL.c0,SEL.c1);
for(var r=rn;r<=rx;r++){
var g=document.getElementById('g'+(r+1));if(!g)continue;
for(var c=cn;c<=cx;c++)g.children[c].classList.add('sel');
}
var hasSel=(rn!==rx||cn!==cx);
document.getElementById('blinkbtn').className=hasSel?'sm':'off sm';
document.getElementById('blinkspd').style.display=hasSel?'inline':'none';
}

function cellAt(x,y){
var el=document.elementFromPoint(x,y);
if(el&&el.dataset&&el.dataset.r!==undefined)return{r:+el.dataset.r-1,c:+el.dataset.c};
return null;
}

function api(m,u,b){
return fetch('/api/'+u,{method:m,headers:b?{'Content-Type':'application/json'}:{},body:b?JSON.stringify(b):undefined})
.then(function(r){return r.json()});
}

function parseLine(raw){
var text='',blinks=[];var i=0;
while(i<raw.length){
var bs=raw.indexOf('[blink]',i);
if(bs===-1){text+=raw.substring(i);for(var j=i;j<raw.length;j++)blinks.push(false);break;}
text+=raw.substring(i,bs);for(var j=i;j<bs;j++)blinks.push(false);
var be=raw.indexOf('[/blink]',bs+7);
if(be===-1){text+=raw.substring(bs+7);for(var j=bs+7;j<raw.length;j++)blinks.push(true);break;}
text+=raw.substring(bs+7,be);for(var j=bs+7;j<be;j++)blinks.push(true);
i=be+8;}
return{text:text,blinks:blinks};
}

function buildLine(r){
var g=document.getElementById('g'+(r+1));
var s='';for(var c=0;c<C;c++)s+=g.children[c].value||' ';
s=s.replace(/\s+$/,'');
var bl=BL[r]||[],out='',inB=false;
for(var c=0;c<s.length;c++){
if(bl[c]&&!inB){out+='[blink]';inB=true;}
if(!bl[c]&&inB){out+='[/blink]';inB=false;}
out+=s[c];}
if(inB)out+='[/blink]';
return out;
}

function load(){
api('GET','state').then(function(d){
S=d;C=d.config.cols;R=d.config.rows;
BL=[];for(var r=0;r<R;r++){BL[r]=[];for(var c=0;c<C;c++)BL[r][c]=false;}
var sel=document.getElementById('pg');sel.innerHTML='';
for(var i=1;i<=d.config.pages;i++){
var o=document.createElement('option');o.value=i;o.text=i;
if(i===P)o.selected=true;sel.appendChild(o);}
sel.onchange=function(){P=+this.value;show()};
show();
});
}

function show(){
var pg=S.pages[P-1];
var b=document.getElementById('bright');b.value=pg.brightness;
document.getElementById('brightval').textContent=pg.brightness;
document.getElementById('time').value=pg.readtime;
document.getElementById('scroll').checked=pg.scroll;
document.getElementById('fade').checked=pg.fade;
var ms=document.getElementById('mspeed');ms.value=pg.move_speed;
document.getElementById('msval').textContent=pg.move_speed;
document.getElementById('bold').checked=pg.bold;
if(pg.blink_speed>0)document.getElementById('blinkspd').value=pg.blink_speed;
BL=[];
for(var r=0;r<R;r++){
BL[r]=[];
var parsed=parseLine(pg.lines[r]||'');
pg._p=pg._p||[];pg._p[r]=parsed;
for(var c=0;c<C;c++)BL[r][c]=!!(parsed.blinks[c]);
}
var grid=document.getElementById('grid');grid.innerHTML='';
for(var r=0;r<R;r++){
var row=document.createElement('div');row.className='r';
var n=document.createElement('span');n.className='n';n.textContent=r+1;
row.appendChild(n);
var g=document.createElement('div');g.className='g';g.id='g'+(r+1);
var val=pg._p[r].text;
for(var c=0;c<C;c++){
var inp=document.createElement('input');
inp.type='text';inp.maxLength=1;inp.autocapitalize=CAP?'characters':'off';
var ch=val[c]||'';
inp.value=(ch===' ')?'':ch;
var cls=inp.value?'':'e';
if(BL[r][c])cls+=' b';
inp.className=cls;
inp.dataset.r=''+(r+1);inp.dataset.c=''+c;
inp.addEventListener('input',onin);
inp.addEventListener('keydown',onkey);
inp.addEventListener('paste',onpaste);
inp.addEventListener('mousedown',onmdown);
inp.addEventListener('mouseenter',onmenter);
inp.addEventListener('touchstart',ontstart,{passive:false});
inp.addEventListener('touchmove',ontmove,{passive:false});
inp.addEventListener('touchend',ontend);
g.appendChild(inp);
}
row.appendChild(g);grid.appendChild(row);
}
clearSel();
}

document.addEventListener('mouseup',function(){
if(SEL.on&&SEL.r0===SEL.r1&&SEL.c0===SEL.c1)clearSel();
});

function onmdown(e){
if(e.shiftKey&&SEL.on){
SEL.r1=+this.dataset.r-1;SEL.c1=+this.dataset.c;applySel();e.preventDefault();return;}
SEL.on=true;SEL.r0=SEL.r1=+this.dataset.r-1;SEL.c0=SEL.c1=+this.dataset.c;applySel();
}
function onmenter(e){
if(!SEL.on||!(e.buttons&1))return;
SEL.r1=+this.dataset.r-1;SEL.c1=+this.dataset.c;applySel();e.preventDefault();
}
function ontstart(e){
var t=e.touches[0];TOUCH.tid=t.identifier;var self=this;
TOUCH.timer=setTimeout(function(){
TOUCH.sel=true;SEL.on=true;
SEL.r0=SEL.r1=+self.dataset.r-1;SEL.c0=SEL.c1=+self.dataset.c;
applySel();},300);
}
function ontmove(e){
if(!TOUCH.sel)return;e.preventDefault();
var t=null;for(var i=0;i<e.touches.length;i++){if(e.touches[i].identifier===TOUCH.tid){t=e.touches[i];break;}}
if(!t)return;
var cell=cellAt(t.clientX,t.clientY);
if(cell){SEL.r1=cell.r;SEL.c1=cell.c;applySel();}
}
function ontend(e){
clearTimeout(TOUCH.timer);
if(TOUCH.sel){TOUCH.sel=false;if(SEL.on&&SEL.r0===SEL.r1&&SEL.c0===SEL.c1)clearSel();}
}

function onin(e){
if(this.value.length>1)this.value=this.value.slice(-1);
this.className=this.value?'':'e';
if(BL[+this.dataset.r-1][+this.dataset.c])this.classList.add('b');
var c=+this.dataset.c;
if(this.value&&c<C-1)this.parentNode.children[c+1].focus();
clearSel();
}
function onkey(e){
var c=+this.dataset.c,r=+this.dataset.r;
if(e.key==='Backspace'&&!this.value&&c>0){
var p=this.parentNode.children[c-1];p.focus();p.select();e.preventDefault();
}else if(e.key==='ArrowLeft'&&c>0){this.parentNode.children[c-1].focus();e.preventDefault();
}else if(e.key==='ArrowRight'&&c<C-1){this.parentNode.children[c+1].focus();e.preventDefault();
}else if(e.key==='ArrowUp'&&r>1){document.getElementById('g'+(r-1)).children[c].focus();e.preventDefault();
}else if(e.key==='ArrowDown'&&r<R){document.getElementById('g'+(r+1)).children[c].focus();e.preventDefault();
}
}
function onpaste(e){
e.preventDefault();
var t=(e.clipboardData||window.clipboardData).getData('text');
var c=+this.dataset.c,g=this.parentNode;
for(var j=0;j<t.length&&c+j<C;j++){g.children[c+j].value=t[j];g.children[c+j].className='';}
}

function getlines(){
var lines=[];for(var r=0;r<R;r++)lines.push(buildLine(r));return lines;
}
function send(){
var st=document.getElementById('status');st.textContent='sending...';
api('POST','page',{
page:P,lines:getlines(),
brightness:+document.getElementById('bright').value,
readtime:parseFloat(document.getElementById('time').value)||5,
blink_speed:+document.getElementById('blinkspd').value,
scroll:document.getElementById('scroll').checked,
fade:document.getElementById('fade').checked,
move_speed:+document.getElementById('mspeed').value,
bold:document.getElementById('bold').checked
}).then(function(d){
st.textContent='ok';S.pages[P-1]=d;
setTimeout(function(){st.textContent=''},2000);
});
}
function reset(){
if(!confirm('Reset all pages?'))return;
api('POST','reset').then(function(){load()});
}
load();
</script>
</body></html>"#;

pub fn register(server: &mut EspHttpServer<'static>) -> anyhow::Result<()> {
    server.fn_handler("/", esp_idf_svc::http::Method::Get, |req| {
        let mut resp = req.into_response(200, None, &[("Content-Type", "text/html")])?;
        resp.write(HTML.as_bytes())?;
        Ok::<(), anyhow::Error>(())
    })?;
    Ok(())
}
