import { mount, Input, Scroll, signal, Text, View, type YNode } from "nsh";
import { Animated, createNativeValue } from "nsh/animated";
import { Image } from "nsh/image";
import { Platform, PixelRatio } from "nsh/device";
import { List } from "nsh/list";
export default function App() {
 const animation=createNativeValue(0);const animated=signal("idle");const network=signal("idle");const image=signal("idle");const long=signal(0);
 const value = signal("hello"); const focus = signal("idle"); const offset = signal(0); const press = signal(0);
 let input!: YNode; let scroll!: YNode; let list!: YNode;
 return <View style={{flex:1,padding:12,gap:6,backgroundColor:"#f3f4f6"}}>
  <Text testID={`status:${value()}:${focus()}:${Math.round(offset())}:${press()}:${long()}:${animated()}:${network()}:${image()}`}>{`value=${value()} focus=${focus()} scroll=${Math.round(offset())} press=${press()}`}</Text>
  <Input ref={input} testID="editor" value={value()} onChangeText={v=>value.set(v)} onFocus={()=>focus.set("focused")} onBlur={()=>focus.set("blurred")} placeholder="type here" style={{height:52,padding:10,fontSize:20,color:"#111827",backgroundColor:"white",borderWidth:1,borderColor:"#aaa"}} />
  <View testID="set-value" onPress={()=>{value.set("controlled 中文"); input.setSelection(0,3);}} style={{height:40,backgroundColor:"#bfdbfe",justifyContent:"center"}}><Text>Set controlled</Text></View>
  <View testID="jump" onPress={()=>{scroll.scrollTo({y:500,animated:false}); list.scrollToIndex({index:80,animated:false});}} style={{height:40,backgroundColor:"#bfdbfe",justifyContent:"center"}}><Text>Jump</Text></View>
  <View testID="press" activeOpacity={0.4} onLongPress={()=>long.set(long()+1)} onPress={()=>press.set(press()+1)} style={{height:40,backgroundColor:"#bbf7d0",justifyContent:"center"}}><Text>Press</Text></View>
  <View style={{flexDirection:"row",gap:10,height:60}}>
   <View testID="animate" onPress={()=>{animation.set(0);Animated.timing(animation,{toValue:120,duration:600}).start().then(done=>animated.set(done?"done":"stopped"));}} style={{width:140,backgroundColor:"#bfdbfe",justifyContent:"center"}}><Text>Animate</Text></View>
   <View testID="fetch" onPress={()=>{network.set("loading");fetch("http://127.0.0.1:18765/body").then(r=>r.text()).then(body=>network.set(body)).catch(e=>network.set(`error:${e.message}`));}} style={{width:140,backgroundColor:"#bfdbfe",justifyContent:"center"}}><Text>Fetch</Text></View>
   <Image testID="image" src="data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAAEUlEQVR4nGP4z8DwH4QZYAwAR8oH+WdZbrcAAAAASUVORK5CYII=" onLoad={()=>image.set("loaded")} onError={e=>image.set(`error:${e.message}`)} style={{width:60,height:60}}/>
   <View testID="animated" style={{width:35,height:35,backgroundColor:"#ef4444",transform:[{translateX:animation}]}}/>
  </View>
  <Text testID={`platform:${Platform.OS}:${Platform.Version}:${PixelRatio.getFontScale()}`}>{`${Platform.OS} ${Platform.Version}`}</Text>
  <Scroll ref={scroll} testID="scroller" onScroll={e=>offset.set(e.y)} style={{height:120,backgroundColor:"#e0e7ff"}}><Each of={Array.from({length:30},(_,i)=>i)}>{i=><Text style={{height:40,fontSize:18}}>{`scroll row ${i()}`}</Text>}</Each></Scroll>
  <List ref={list} testID="list" each={Array.from({length:200},(_,i)=>i)} estimatedItemSize={44} style={{flex:1,backgroundColor:"#fef3c7"}}>{i=><Text testID={`row-${i()}`} style={{height:44,fontSize:18}}>{`virtual row ${i()}`}</Text>}</List>
 </View>;
}

mount(App);
