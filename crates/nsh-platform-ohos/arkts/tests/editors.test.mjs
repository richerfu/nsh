import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { stripTypeScriptTypes } from 'node:module';
import test from 'node:test';

// Execute the production owner and callbacks; only SDK rendering effects are substituted.
const source = await readFile(new URL('../src/main/ets/NativeEditors.ets', import.meta.url), 'utf8');
const event = source.slice(source.indexOf('class EditorEvent'), source.indexOf('class EditorModifier'));
const classes = source.slice(source.indexOf('class EditorSizeCallback'));
const effects = `
const quarantinedEditors = new Set();
class FrameCallback {}
class TextInputController { selections=[]; setTextSelection(a,b){this.selections.push([a,b]);} getTextContentRect(){return {width:10,height:20};} stopEditing(){} }
class TextAreaController extends TextInputController {}
class BuilderNode { updates=0; disposals=0; build(){globalThis.sdkBuild?.();} update(){this.updates++; globalThis.sdkUpdate?.();} dispose(){this.disposals++;} getFrameNode(){return {};} }
const editorContent = () => {};
const wrapBuilder = value => value;
`;
const { EditorEntry, NativeEditors } = await import(`data:text/javascript;base64,${Buffer.from(stripTypeScriptTypes(effects+event+classes+'\nexport { EditorEntry };')).toString('base64')}`);
function fixture(id=1, ready=true) {
  const calls=[];
  let active=true, accepted=true, count=0;
  const ui={ postFrameCallback(){}, getFocusController(){return {requestFocus(){globalThis.sdkFocus?.();}}} };
  const context={getUIContext:()=>ui, isActive:()=>active, invokeNativeSync(name,_req,_res,value){
    calls.push({name,value});
    if(name==='editor-event' && value.event.kind==='change') count++;
    return {accepted,count};
  }};
  const settings={targetId:id, multiline:false, text:'initial',count:0};
  const entry=new EditorEntry(settings,context);
  if (ready) { entry.create(); calls.length=0; }
  return {entry,settings,calls,context,accept(value){accepted=value;},active(value){active=value;}};
}
test('full strings including NUL cross one SDK callback and count comes from Rust receipt',()=>{
  const f=fixture(); f.entry.change('a\0😀');
  assert.equal(f.calls[0].value.event.text,'a\0😀');
  assert.equal(f.entry.count,1);
  f.entry.update({...f.settings,text:'stale',count:0});
  assert.equal(f.entry.text,'a\0😀');
  f.entry.update({...f.settings,text:'accepted',count:1});
  assert.equal(f.entry.text,'accepted');
});
test('controlled updates reuse BuilderNode and controllers; callback echo is suppressed',()=>{
  const f=fixture(), node=f.entry.node, controller=f.entry.line;
  globalThis.sdkUpdate=()=>f.entry.change('programmatic echo');
  try {f.entry.update({...f.settings,text:'controlled'});}finally{globalThis.sdkUpdate=undefined;}
  assert.equal(f.entry.node,node); assert.equal(f.entry.line,controller);
  assert.equal(node.updates,1); assert.equal(f.entry.text,'controlled'); assert.equal(f.calls.length,0);
  f.entry.command({kind:'selection',start:1,end:3});
  assert.deepEqual(controller.selections,[[1,3]]);
});
test('SDK update failure clears applying guard and is observable',()=>{
  const f=fixture(); globalThis.sdkUpdate=()=>{throw Error('SDK failed');};
  try {assert.throws(()=>f.entry.update({...f.settings,text:'wanted'}),/SDK failed/);}finally{globalThis.sdkUpdate=undefined;}
  f.entry.change('user'); assert.equal(f.entry.count,1);
});
test('retirement failure retains SDK owner and permits a later successful retry',()=>{
  const f=fixture(); f.accept(false);
  assert.throws(()=>f.entry.dispose(),/retirement/); assert.equal(f.entry.node.disposals,0);
  f.accept(true); f.entry.dispose(); f.entry.dispose(); assert.equal(f.entry.node.disposals,1);
  assert.equal(f.calls.filter(c=>c.name==='native-node-retiring').length,2);
  const events=f.calls.length; f.entry.change('late'); f.entry.emit('focus'); assert.equal(f.calls.length,events);
});
test('synchronous close during SDK build retires before returning and does not attach later',()=>{
  const f=fixture(), editors=new NativeEditors();
  globalThis.sdkBuild=()=>editors.dispose(f.context);
  try {assert.throws(()=>editors.update([f.settings],[],f.context),/cancelled/);}finally{globalThis.sdkBuild=undefined;}
  assert.deepEqual(f.calls.map(c=>c.name),['native-node-retiring']);
});
test('retiring one SDK instance cannot clear another instance',()=>{
  const first=fixture(11),second=fixture(12),a=new NativeEditors(),b=new NativeEditors();
  a.update([first.settings],[],first.context); b.update([second.settings],[],second.context);
  a.dispose(first.context); const before=second.calls.length;
  b.update([{...second.settings,text:'still live'}],[],second.context);
  assert.equal(second.calls.length,before);
  b.dispose(second.context);
});

test('initial autofocus waits for native attachment and synchronous focus is delivered',()=>{
  const f=fixture(21,false); f.entry.settings.autoFocus=true;
  globalThis.sdkBuild=()=>f.entry.appeared();
  globalThis.sdkFocus=()=>f.entry.emit('focus');
  try {f.entry.create();} finally {globalThis.sdkBuild=undefined;globalThis.sdkFocus=undefined;}
  assert.deepEqual(f.calls.map(c=>c.name),['native-node-ready','editor-event']);
  assert.equal(f.calls[1].value.event.kind,'focus');
});

test('soft keyboard insertions and Backspace produce one key event per character',()=>{
  const f=fixture();
  f.entry.editKey('a😀'); f.entry.editKey('Backspace');
  assert.deepEqual(f.calls.map(c=>c.value.event.text),['a','😀','Backspace']);
});
test('controlled updates do not emit soft-keyboard events',()=>{
  const f=fixture();
  globalThis.sdkUpdate=()=>{f.entry.editKey('x');f.entry.editKey('Backspace');};
  try {f.entry.update({...f.settings,text:'controlled'});} finally {globalThis.sdkUpdate=undefined;}
  assert.deepEqual(f.calls,[]);
});
