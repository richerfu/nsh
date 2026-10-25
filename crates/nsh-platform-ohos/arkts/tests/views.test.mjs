import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {stripTypeScriptTypes} from 'node:module';
import test from 'node:test';
const base=new URL('../src/main/ets/',import.meta.url);
const source=await readFile(new URL('NativeViews.ets',base),'utf8');
const pointer=await readFile(new URL('NativePointer.ets',base),'utf8');
const state=source.slice(source.indexOf('class ViewState'),source.indexOf('@Component',source.indexOf('class ViewState')));
const effects=`
const quarantinedViews=new Set();
class NodeContent {}
class BuilderNode { build(){globalThis.sdkBuild?.();} update(){globalThis.sdkUpdate?.();} dispose(){globalThis.effects.push('dispose');} getFrameNode(){return {};} }
class NativeMenuCleanup {async settled(){}}
const sourceContent=()=>{},textureContent=()=>{},viewContent=()=>{},wrapBuilder=v=>v;
const deviceInfo={sdkApiVersion:26};
`;
const classes=source.slice(source.indexOf('class ViewEntry'));
const {NativeViews}=await import(`data:text/javascript;base64,${Buffer.from(stripTypeScriptTypes(pointer+effects+state+classes)).toString('base64')}`);
function fixture() {
  globalThis.effects=[];
  const context={getUIContext:()=>({}),isActive:()=>true,invokeNativeSync(name){globalThis.effects.push(name);return {accepted:true};}};
  return {owner:new NativeViews(),context,settings:{targetId:1,width:10,height:20}};
}
test('synchronous close during BuilderNode build detaches before deferred SDK disposal',()=>{
  const f=fixture(); globalThis.sdkBuild=()=>{globalThis.effects.push('build');f.owner.dispose(f.context);globalThis.effects.push('build-return');};
  try {assert.throws(()=>f.owner.update([f.settings],f.context),/cancelled/);}finally{globalThis.sdkBuild=undefined;}
  assert.deepEqual(globalThis.effects,['build','native-node-retiring','build-return','dispose']);
});
test('synchronous close during update defers SDK release until update returns',()=>{
  const f=fixture(); f.owner.update([f.settings],f.context);globalThis.effects=[];
  globalThis.sdkUpdate=()=>{globalThis.effects.push('update');f.owner.dispose(f.context);globalThis.effects.push('update-return');};
  try {f.owner.update([{...f.settings,width:30}],f.context);}finally{globalThis.sdkUpdate=undefined;}
  assert.deepEqual(globalThis.effects,['update','native-node-retiring','update-return','dispose']);
});
test('construction errors identify the target and SDK build or native attachment stage',()=>{
  const build=fixture();globalThis.sdkBuild=()=>{throw new Error('ParamInvalid');};
  try {assert.throws(()=>build.owner.update([build.settings],build.context),/wrapper 1 build failed: Error: ParamInvalid/);}
  finally {globalThis.sdkBuild=undefined;}
  const attach=fixture();
  attach.context.invokeNativeSync=(name)=>{if(name==='native-subtree-ready'){throw new Error('ParamInvalid');}return {accepted:true};};
  assert.throws(()=>attach.owner.update([attach.settings],attach.context),/wrapper 1 attach failed: Error: ParamInvalid/);
});
