// Browser checks against the production UI with isolated HTTP/WebSocket fixtures.
// node scripts/check-ui.cjs <playwright-module> [browser-executable] [base-url]
const { chromium } = require(process.argv[2] || 'playwright');
const assert = require('node:assert/strict');
const path = require('node:path');
const fs = require('node:fs');
const base=process.argv[4] || 'http://127.0.0.1:4189';
const ref={group:'',version:'v1',plural:'pods'};
const resource={...ref,kind:'Pod',singular:'pod',shortNames:['po'],namespaced:true,verbs:['list','watch','get','update','delete'],subresources:['log','exec']};
const columns=['Name','Ready','Status','Restarts','Age'].map((name,i)=>({name,type:'string',format:i===0?'name':'',description:'',priority:0}));
const created=new Date(Date.now()-1200000).toISOString();
const rows=['CrashLoopBackOff','Pending','Completed','Running'].map((status,i)=>({key:String(i),name:['emails','inventory','finished-job','api'][i],namespace:'shop',created,labels:{app:['emails','inventory','job','api'][i]},owners:[],cells:[['emails','inventory','finished-job','api'][i],status==='Running'?'1/1':'0/1',status,status==='CrashLoopBackOff'?8:0,'20m']}));
const pod={apiVersion:'v1',kind:'Pod',metadata:{name:'emails',namespace:'shop',uid:'pod-1',resourceVersion:'10',creationTimestamp:created,labels:{app:'emails'}},spec:{nodeName:'worker-02',containers:[{name:'emails',image:'emails:1.0'}]},status:{phase:'Running',containerStatuses:[{name:'emails',ready:false,restartCount:8,state:{waiting:{reason:'CrashLoopBackOff'}},lastState:{terminated:{exitCode:1,reason:'Error'}}}]}};
const event={kind:'Warning',reason:'BackOff',message:'Back-off restarting failed container',count:8,source:'kubelet',lastSeen:created,objectKind:'Pod',object:{resource:ref,namespace:'shop',name:'emails'}};
const pulse={cpu:{used:2700,requested:2000,capacity:8000},memory:{used:900000000,requested:800000000,capacity:1600000000},kinds:[{kind:'Pod',resource:ref,total:4,ok:2,warning:1,failed:1,error:null}],events:{normal:4,warning:8,recent:[event,event],error:null}};
const k9s={dirs:[],files:[],aliases:[],hotkeys:[],plugins:[],skins:[],skin:null,readOnly:false,defaultView:null,thresholds:{cpu:{warn:70,critical:90},memory:{warn:70,critical:90}},logger:{tail:100,sinceSeconds:-1,showTime:false,textWrap:false,disableAutoscroll:false},portForwardAddress:null,jumps:[]};
(async()=>{
 const browser=await chromium.launch({headless:true,...(process.argv[3]?{executablePath:process.argv[3]}:{})});
 const page=await browser.newPage({viewport:{width:1440,height:900}});
 const errors=[], mutations=[];let failSave=false, context='test-cluster', tableSocket;
 page.on('pageerror',e=>errors.push(e.message));
 await page.route('**/api/**',async route=>{
  const req=route.request(),url=new URL(req.url()),p=url.pathname.replace('/api','');let data=[];let status=200;
  if(p==='/session')data={signedIn:true,required:false};
  if(p==='/contexts'||p==='/contexts/reload')data={contexts:[{name:context,cluster:'test',namespace:'shop',user:null,isDefault:true}],cluster:{context,server:'https://fixture.invalid',version:'v1.36.0',defaultNamespace:'shop'}};
  if(p==='/context'){context=JSON.parse(req.postData()).name;data={context,server:'https://fixture.invalid',version:'v1.36.0',defaultNamespace:'shop'};}
  if(p==='/resources')data=[resource,{...resource,plural:'namespaces',kind:'Namespace',shortNames:['ns'],namespaced:false}];
  if(p==='/k9s')data=k9s;
  if(p==='/pulse')data=pulse;
  if(p==='/object'&&req.method()==='GET')data=pod;
  if(p==='/events')data=[event];
  if(p==='/metrics')data=rows.map(r=>({name:r.name,namespace:'shop',cpu:20,memory:10485760,requests:{cpu:100,memory:104857600},limits:{cpu:1000,memory:1048576000}}));
  if(p==='/object'&&req.method()!=='GET'){mutations.push({method:req.method(),body:JSON.parse(req.postData())});if(failSave){status=409;data={error:'Object changed on server'};}else data=pod;}
  await route.fulfill({status,contentType:'application/json',body:JSON.stringify(data)});
 });
 await page.routeWebSocket('**/api/table?**',socket=>{
  const ns=new URL(socket.url()).searchParams.get('plural')==='namespaces';
  if(!ns)tableSocket=socket;
  socket.send(JSON.stringify({type:'reset',columns:ns?[columns[0]]:columns,rows:ns?[{...rows[0],name:'shop',cells:['shop']}]:rows}));
 });
 await page.routeWebSocket('**/api/logs',socket=>{socket.onMessage(()=>{socket.send(JSON.stringify({type:'sources',sources:[{pod:'emails',container:'emails'}]}));socket.send(JSON.stringify({type:'lines',lines:[{source:0,text:'ERROR SMTP connection refused'}]}));socket.send(JSON.stringify({type:'end'}));});});
 const out=path.resolve(__dirname,'../docs/design-review/application');fs.mkdirSync(out,{recursive:true});
 await page.goto(base); await page.getByRole('heading',{name:'Cluster overview',exact:true}).waitFor();
 await page.locator('.signal').first().waitFor();assert.equal(await page.locator('.signal').count(),1);
 await page.screenshot({path:path.join(out,'overview.png')});
 await page.locator('.signal').click();
 await page.getByRole('button',{name:'Read previous container logs',exact:true}).click();
 await page.getByText('ERROR SMTP connection refused',{exact:false}).waitFor();
 assert.match(await page.locator('.stream-state').textContent(),/Previous container/);
 await page.screenshot({path:path.join(out,'logs.png')});
 await page.getByRole('button',{name:/^YAML/}).click();
 await page.getByRole('button',{name:/^Edit /}).click();
 const editor=page.locator('.cm-content[contenteditable=true]');await editor.waitFor();
 const original=await editor.innerText();await editor.fill(original.replace('emails:1.0','emails:2.0'));
 await page.getByRole('button',{name:'Close',exact:true}).click();
 await page.getByRole('dialog',{name:'Keep your YAML changes?'}).waitFor();
 await page.getByRole('button',{name:'Keep editing',exact:true}).click();
 assert.match(await editor.innerText(),/emails:2.0/);
 await page.getByRole('button',{name:'Overview d',exact:true}).click();
 failSave=true;await page.getByRole('button',{name:'Save and continue',exact:true}).click();
 await page.waitForFunction(()=>document.querySelector('.cm-content')?.textContent.includes('emails:2.0'));
 assert.equal(await page.getByRole('dialog',{name:'Keep your YAML changes?'}).count(),1);
 await page.getByRole('button',{name:'Keep editing',exact:true}).click();
 failSave=false;await page.getByRole('button',{name:'Close',exact:true}).click();await page.getByRole('button',{name:'Discard changes',exact:true}).click();
 await page.getByRole('button',{name:'Pods',exact:true}).click();
 await page.getByRole('button',{name:'emails',exact:true}).click();
 await page.screenshot({path:path.join(out,'pods.png')});
 await page.getByRole('button',{name:'Close',exact:true}).click();
 await page.keyboard.press('z');await page.getByRole('dialog',{name:'Delete Pod',exact:true}).waitFor();
 assert.match(await page.locator('dialog .targets').textContent(),/finished-job/);
 assert.doesNotMatch(await page.locator('dialog .targets').textContent(),/emails|inventory/);
 assert.equal(await page.evaluate(()=>document.activeElement?.textContent),'Cancel');
 await page.keyboard.press('Enter');assert.equal(mutations.filter(m=>m.method==='DELETE').length,0);
 await page.getByRole('button',{name:'Pods',exact:true}).click();
 await page.getByRole('button',{name:'emails',exact:true}).click();
 for(const width of [1440,1280,900,768,390,320]){
  await page.setViewportSize({width,height:width===900?560:900});
  const size=await page.evaluate(()=>({doc:document.documentElement.scrollWidth,win:innerWidth}));assert.ok(size.doc<=width+1,`Inspector overflow at ${width}: ${size.doc}`);
 }
 await page.screenshot({path:path.join(out,'inspector-mobile.png'),fullPage:true});
 await page.getByRole('button',{name:'Close',exact:true}).click();
 for(const width of [900,390,320]){
  await page.setViewportSize({width,height:900});
  const size=await page.evaluate(()=>document.documentElement.scrollWidth);assert.ok(size<=width+1,`List overflow at ${width}: ${size}`);
 }
 await page.setViewportSize({width:1440,height:900});
 tableSocket.send(JSON.stringify({type:'error',message:'403 Forbidden'}));
 await page.getByText(/^Connection interrupted. Showing the rows from/).waitFor();
 assert.ok(await page.locator('tbody tr').count()>0);
 assert.deepEqual(errors,[]);
 await browser.close();console.log('PASS: production UI navigation, warning grouping, previous logs, dirty-edit exits, failed saves, cleanup, stale data, and responsive layouts.');
})().catch(e=>{console.error(e);process.exit(1);});
