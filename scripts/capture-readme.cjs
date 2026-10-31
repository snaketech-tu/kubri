// Capture the built application with synthetic cluster data; no kubeconfig is read.
// node scripts/capture-readme.cjs <playwright-module> [browser-executable] [base-url]
const { chromium } = require(process.argv[2] || 'playwright');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { version } = require('../ui/package.json');
const base = process.argv[4] || 'http://127.0.0.1:4190';
const out = path.resolve(__dirname, '../docs/assets/screenshots');
const clock = new Date('2026-10-31T12:00:00Z');
const ago = (minutes) => new Date(+clock - minutes * 60000).toISOString();
const ref = (plural, group = '') => ({ group, version: 'v1', plural });
const kinds = [
  ['pods','Pod','', ['po']], ['deployments','Deployment','apps',['deploy']],
  ['statefulsets','StatefulSet','apps',['sts']], ['daemonsets','DaemonSet','apps',['ds']],
  ['jobs','Job','batch',[]], ['cronjobs','CronJob','batch',['cj']],
  ['services','Service','',['svc']], ['ingresses','Ingress','networking.k8s.io',['ing']],
  ['configmaps','ConfigMap','',['cm']], ['secrets','Secret','',[]],
  ['persistentvolumeclaims','PersistentVolumeClaim','',['pvc']],
  ['nodes','Node','',['no']], ['namespaces','Namespace','',['ns']], ['events','Event','',['ev']],
];
const resources = kinds.map(([plural,kind,group,shortNames]) => ({...ref(plural,group),kind,singular:kind.toLowerCase(),shortNames,namespaced:!['nodes','namespaces'].includes(plural),verbs:['get','list','watch','update','delete'],subresources:['pods','deployments'].includes(plural)?['log','exec','scale']:[]}));
const podData = [
  ['api-7965c47658-hqfpv','Running',0,28,104,'api'],
  ['api-7965c47658-tshmp','Running',0,32,112,'api'],
  ['api-7965c47658-xkvtq','Running',0,24,96,'api'],
  ['cache-685bd955bf-q5gk4','Running',0,12,86,'cache'],
  ['cart-58b45b48bc-dxngd','Running',0,18,74,'cart'],
  ['emails-99d4775f6-56lmg','CrashLoopBackOff',8,null,null,'emails'],
  ['frontend-d45c57b5b-lhcx2','Running',0,9,68,'frontend'],
  ['frontend-d45c57b5b-ntfb6','Running',0,8,64,'frontend'],
  ['inventory-86db8c5b7-p9w2k','Pending',0,null,null,'inventory'],
  ['orders-db-0','Running',0,112,384,'orders-db'],
  ['payments-6556cb8d8b-6kwsk','Running',0,38,128,'payments'],
  ['payments-6556cb8d8b-7rmtb','Running',0,42,132,'payments'],
  ['search-indexer-bb5445b4-kzctl','Running',0,200,256,'search-indexer'],
  ['web-79bb5b8f89-s5k2x','Running',0,22,80,'web'],
  ['web-79bb5b8f89-tb7kp','Running',0,18,78,'web'],
  ['worker-7dbd94fbf7-zvlh5','Running',0,58,152,'worker'],
  ['worker-7dbd94fbf7-td5w8','Running',0,46,140,'worker'],
];
const columns = ['Name','Ready','Status','Restarts','Age'].map((name,i) => ({name,type:'string',format:i===0?'name':'',description:'',priority:0}));
const rows = podData.map(([name,status,restarts,cpu,mem,app],i) => ({key:`demo-pod-${i}`,name,namespace:'shop',created:ago(19),labels:{app},owners:[{uid:`rs-${app}`,name:`${app}-7965c47658`,kind:'ReplicaSet',apiVersion:'apps/v1',controller:true}],cells:[name,status==='Running'?'1/1':'0/1',status,restarts,'19m']}));
const event = (name,reason,message,count,minutes) => ({kind:'Warning',reason,message,count,source:'kubelet',lastSeen:ago(minutes),objectKind:'Pod',object:{resource:ref('pods'),namespace:'shop',name}});
const events = [
  event(podData[5][0],'BackOff','Back-off restarting failed container emails',8,2),
  event(podData[8][0],'FailedScheduling','0/3 nodes are available: insufficient memory',3,4),
  event(podData[5][0],'Unhealthy','Readiness probe failed: connection refused on port 8080',2,6),
];
const pulseKinds = [['Pod',17,15,1,1],['Node',3,3,0,0],['Deployment',12,11,0,1],['StatefulSet',1,1,0,0],['DaemonSet',2,2,0,0],['Job',2,2,0,0],['CronJob',1,1,0,0]].map(([kind,total,ok,warning,failed]) => ({kind,resource:resources.find(r=>r.kind===kind),total,ok,warning,failed,error:null}));
const k9s = {dirs:[],files:[],aliases:[],hotkeys:[],plugins:[],skins:[],skin:null,readOnly:false,defaultView:null,thresholds:{cpu:{warn:70,critical:90},memory:{warn:70,critical:90}},logger:{tail:100,sinceSeconds:-1,showTime:true,textWrap:true,disableAutoscroll:false},portForwardAddress:null,jumps:[]};
function object(name) {
  const row=podData.find(p=>p[0]===name)||podData[0];
  const failed=row[1]==='CrashLoopBackOff';
  return {apiVersion:'v1',kind:'Pod',metadata:{name:row[0],namespace:'shop',uid:`uid-${name}`,resourceVersion:'42',creationTimestamp:ago(19),labels:{app:row[5]},ownerReferences:[{apiVersion:'apps/v1',kind:'ReplicaSet',name:`${row[5]}-99d4775f6`,uid:`rs-${row[5]}`,controller:true}]},spec:{nodeName:'worker-eu-02',serviceAccountName:'default',containers:[{name:row[5],image:`ghcr.io/example/${row[5]}:1.8.2`}]},status:{phase:row[1]==='Pending'?'Pending':'Running',podIP:'10.42.0.62',qosClass:'Burstable',conditions:[{type:'Ready',status:failed?'False':'True',reason:failed?'ContainersNotReady':'',lastTransitionTime:ago(2)}],containerStatuses:[{name:row[5],ready:!failed,restartCount:row[2],state:failed?{waiting:{reason:'CrashLoopBackOff'}}:{running:{startedAt:ago(19)}},lastState:failed?{terminated:{exitCode:1,reason:'Error',finishedAt:ago(2)}}:{}}]}};
}
const tree = ['api','emails','frontend','payments','worker'].map((app,i) => ({id:app,kind:'Deployment',name:app,namespace:'shop',resource:ref('deployments','apps'),health:app==='emails'?'failed':'ok',detail:app==='emails'?'0/1 replicas ready':'2/2 replicas ready',children:[{id:`${app}/rs`,kind:'ReplicaSet',name:`${app}-7965c47658`,namespace:'shop',resource:null,health:app==='emails'?'failed':'ok',detail:'revision 12',children:rows.filter(r=>r.labels.app===app).map(r=>({id:r.key,kind:'Pod',name:r.name,namespace:'shop',resource:ref('pods'),health:r.cells[2]==='Running'?'ok':'failed',detail:String(r.cells[2]),children:[{id:`${r.key}/container`,kind:'Container',name:app,namespace:'shop',resource:null,health:r.cells[2]==='Running'?'ok':'failed',detail:`${r.cells[3]} restarts`,children:[]}]}))}]}));

(async () => {
  const browser = await chromium.launch({headless:true,...(process.argv[3]?{executablePath:process.argv[3]}:{})});
  try {
    const page = await browser.newPage({viewport:{width:1600,height:1000},deviceScaleFactor:1,colorScheme:'light'});
    const errors=[];let samples=0;
    page.on('pageerror',e=>errors.push(e.message));
    await page.clock.install({time:clock});
    await page.addInitScript(() => {
      localStorage.setItem('kubri.readonly.demo-eu','true');
      localStorage.setItem('kubri.environment.demo-eu','Demo');
    });
    await page.route('**/api/**',async route => {
      const request=route.request(),url=new URL(request.url()),p=url.pathname.slice(4);
      let data=[];
      const cluster={context:'demo-eu',server:'https://cluster.example.invalid',version:'v1.36.0',defaultNamespace:'shop'};
      if(p==='/session')data={signedIn:true,required:false};
      if(p==='/contexts'||p==='/contexts/reload')data={contexts:[{name:'demo-eu',cluster:'demo',user:null,namespace:'shop',isDefault:true}],cluster};
      if(p==='/resources')data=resources;
      if(p==='/k9s')data=k9s;
      if(p==='/object')data=object(url.searchParams.get('name'));
      if(p==='/events')data=events.filter(e => `uid-${e.object.name}` === url.searchParams.get('uid'));
      if(p==='/metrics')data=podData.filter(r=>r[3]!==null).map(r=>({name:r[0],namespace:'shop',cpu:r[3],memory:r[4]*1048576,requests:{cpu:125,memory:256*1048576},limits:{cpu:1000,memory:1024*1048576}}));
      if(p==='/pulse'){
        const cpu=[2160,2040,2280,2440,2320,2560,2480,2640,2720][Math.min(samples++,8)];
        data={cpu:{used:cpu,requested:3200,capacity:8000},memory:{used:Math.round((9.5+Math.min(samples,8)*.05)*1073741824),requested:8*1073741824,capacity:16*1073741824},kinds:pulseKinds,events:{normal:99,warning:13,recent:events,error:null}};
      }
      if(p==='/xray')data=tree;
      if(!['GET','HEAD'].includes(request.method()) && p!=='/contexts/reload')throw new Error(`Screenshot attempted a mutation: ${request.method()} ${p}`);
      await route.fulfill({status:200,contentType:'application/json',body:JSON.stringify(data)});
    });
    await page.routeWebSocket('**/api/table?**',socket => {
      const plural=new URL(socket.url()).searchParams.get('plural');
      const data=plural==='namespaces'?['shop','kube-system'].map(name=>({key:name,name,namespace:null,created:ago(1440),labels:{},owners:[],cells:[name]})):rows;
      socket.send(JSON.stringify({type:'reset',columns:plural==='namespaces'?[columns[0]]:columns,rows:data}));
    });
    await page.routeWebSocket('**/api/logs',socket => socket.onMessage(() => {
      socket.send(JSON.stringify({type:'sources',sources:[{pod:podData[5][0],container:'emails'}]}));
      socket.send(JSON.stringify({type:'lines',lines:[
        '2026-10-31T11:58:01Z INFO  Booting email worker v1.8.2',
        '2026-10-31T11:58:01Z INFO  Configuration loaded: namespace=shop',
        '2026-10-31T11:58:01Z INFO  Queue consumer connected: email.delivery',
        '2026-10-31T11:58:02Z INFO  Initializing SMTP transport: smtp:587',
        '2026-10-31T11:58:02Z ERROR connect ECONNREFUSED 10.43.18.42:587',
        '2026-10-31T11:58:02Z WARN  Retry 1/3 in 1000ms',
        '2026-10-31T11:58:03Z ERROR connect ECONNREFUSED 10.43.18.42:587',
        '2026-10-31T11:58:03Z WARN  Retry 2/3 in 2000ms',
        '2026-10-31T11:58:05Z ERROR connect ECONNREFUSED 10.43.18.42:587',
        '2026-10-31T11:58:05Z WARN  Retry 3/3 in 4000ms',
        '2026-10-31T11:58:09Z ERROR SMTP transport unavailable after 3 retries',
        '2026-10-31T11:58:09Z ERROR Startup aborted. Process exited with code 1.',
      ].map(text=>({source:0,text}))}));
      socket.send(JSON.stringify({type:'end'}));
    }));
    fs.mkdirSync(out,{recursive:true});
    const shot=async name => { await page.screenshot({path:path.join(out,name),animations:'disabled'}); console.log(name); };
    await page.goto(base);
    await page.getByRole('heading',{name:'Cluster overview',exact:true}).waitFor();
    await page.getByLabel(`Version ${version}`,{exact:true}).waitFor();
    await page.locator('.signal').first().waitFor();
    for(let i=0;i<8;i++)await Promise.all([page.waitForResponse(r=>new URL(r.url()).pathname==='/api/pulse'),page.clock.fastForward(10000)]);
    await page.setViewportSize({width:1600,height:1120});
    await shot('overview.png');
    await page.setViewportSize({width:1600,height:1000});
    await page.getByRole('button',{name:'Pods',exact:true}).click();
    await page.locator('tbody tr').first().waitFor();
    await page.getByRole('combobox').last().selectOption('shop');
    await shot('pods-light.png');
    await page.getByRole('button',{name:'Use dark colors',exact:true}).click();
    await shot('pods-dark.png');
    await page.getByRole('button',{name:'Use light colors',exact:true}).click();
    await page.getByRole('button',{name:podData[5][0],exact:true}).click();
    await page.getByRole('button',{name:'Read previous container logs',exact:true}).waitFor();
    await shot('describe.png');
    await page.getByRole('button',{name:'Read previous container logs',exact:true}).click();
    await page.getByText('Startup aborted.',{exact:false}).waitFor();
    await page.getByRole('button',{name:'Maximize',exact:true}).click();
    await page.getByRole('button',{name:'Use dark colors',exact:true}).click();
    await shot('logs-previous.png');
    await page.getByRole('button',{name:'Use light colors',exact:true}).click();
    await page.getByRole('button',{name:'Close',exact:true}).click();
    await page.getByRole('button',{name:'Topology XRay',exact:true}).click();
    await page.locator('.row').first().waitFor();
    await shot('xray.png');
    await page.getByRole('button',{name:'Pods',exact:true}).click();
    await page.getByRole('button',{name:'Search or run a command',exact:true}).click();
    await page.locator('.palette input').fill('deploy');
    await page.locator('.palette li').first().waitFor();
    await shot('palette.png');
    assert.deepEqual(errors,[]);
    console.log(`Captured 7 README screenshots from Kubri v${version}; no JavaScript errors.`);
  } finally { await browser.close(); }
})().catch(e=>{console.error(e);process.exit(1);});
