import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {stripTypeScriptTypes} from 'node:module';
import test from 'node:test';
const source=await readFile(new URL('../src/main/ets/NshPlugin.ets',import.meta.url),'utf8');
const effects=`
const deviceInfo={deviceType:'2in1'};
const expectBridgeRequestType=()=>{};
class NativeEditors { remove(){globalThis.steps.push('remove-editors');} update(){globalThis.steps.push('editors');globalThis.onEditors?.();} dispose(){} }
class NativeViews { remove(){globalThis.steps.push('remove-views');} update(){globalThis.steps.push('views');} dispose(){} async settled(){} }
`;
const {NshPlugin}=await import(`data:text/javascript;base64,${Buffer.from(stripTypeScriptTypes(effects+source.slice(source.indexOf('export class NshPlugin')))).toString('base64')}`);
function fixture(cacheDir='/data/storage/el2/base/cache'){
  globalThis.steps=[];
  let ready=false,active=true;
  const events=[];
  const context={abilityContext:{cacheDir},isActive:()=>active,getUIContext(){if(!ready)throw Error('UI not ready');return{};},invokeNativeSync(...args){events.push(args);return{accepted:true};}};
  const plugin=new NshPlugin();plugin.attachContext(context);
  return{plugin,context,events,ready(){ready=true;},close(){active=false;}};
}
const batch={value:{editors:[],commands:[],views:[],removeEditors:[],removeViews:[]}};
test('Ability-level install sends device type and private cache directory before any UI exists',()=>{
  const f=fixture();assert.deepEqual(f.plugin.requires,['ability']);f.plugin.onInstall(f.context);
  assert.deepEqual(f.events,[['device-info','nsh.arkui.DeviceInfo','ohos.node.Acknowledgement',{deviceType:'2in1',cacheDir:'/data/storage/el2/base/cache'}]]);
  assert.throws(()=>f.plugin.invokeSync('apply',batch,f.context),/UI not ready/);
  assert.deepEqual(globalThis.steps,[]);
});
test('install uses the cache directory of its own Ability context',()=>{
  const a=fixture('/cache/first'),b=fixture('/cache/second');
  a.plugin.onInstall(a.context);b.plugin.onInstall(b.context);
  assert.equal(a.events[0][3].cacheDir,'/cache/first');
  assert.equal(b.events[0][3].cacheDir,'/cache/second');
});
test('SDK apply proceeds only after actual UIContext is ready',()=>{
  const f=fixture();f.ready();assert.equal(f.plugin.invokeSync('apply',batch,f.context).value.accepted,true);
  assert.deepEqual(globalThis.steps,['remove-editors','remove-views','editors','views']);
});
test('reentrant closing prevents later phases of the same batch from constructing owners',()=>{
  const f=fixture();f.ready();globalThis.onEditors=()=>f.close();
  try{assert.throws(()=>f.plugin.invokeSync('apply',batch,f.context),/inactive/);}finally{globalThis.onEditors=undefined;}
  assert.deepEqual(globalThis.steps,['remove-editors','remove-views','editors']);
});
