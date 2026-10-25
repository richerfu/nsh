---
title: OHOS
description: 在 OHOS ArkUI 宿主中嵌入 Native Shell 应用，每个 surface 使用独立引擎。
order: 13
group: platforms
---

# OHOS

Native Shell 通过 ArkUI 自定义节点、Taffy 布局和原生文本排版，把模块嵌入 OHOS 应用。arm64 后端以 API 20 为适配基线。每个 `NshSurface` 独立持有平台 `NodeContent`、引擎、JS VM 和 VSync。
业务通过标准 `ContentSlot` 放置实例，并像 iOS、Android 嵌入视图一样管理其布局和生命周期。

## 环境要求

- **DevEco Studio**、带原生工具链的 HarmonyOS SDK、OHPM、Hvigor，以及 arm64 真机或
  模拟器。下面的命令以 macOS 为例。
- **Rust**、`aarch64-unknown-linux-ohos` target 和 **ohrs**，用于从 NSH 源码构建 SDK。
- 根 Cargo manifest 锁定的 Ability 和原生 binding 源码。Cargo 获取准确的 Git revision，
  SDK metadata 将 Rust 与 HAR 来源配对。参见
  [binding 配置](../../../docs/dev/ohos.md#locked-native-dependencies)。

示例的兼容和目标 SDK 版本均为 `6.0.0(20)`。构建和 NDK 导入符号检查覆盖 API 20
基线，运行验证记录来自 API 26 arm64 模拟器。具体范围见
[验证记录](../../../docs/dev/ohos.md#verification)。

## 获取原生库

在 NSH 仓库根目录执行：

```sh
rustup target add aarch64-unknown-linux-ohos
cargo install ohrs --locked
export DEVECO_CONTENTS=/path/to/DevEco-Studio.app/Contents
export OHOS_NDK_HOME="$DEVECO_CONTENTS/sdk/default/openharmony"
cargo xtask ohos
```

生成的 SDK 目录为：

```text
target/ohos/debug/
  arm64-v8a/libnsh_platform_ohos.so
  index.d.ts
  build-metadata.json
  native-imports.json
```

`cargo xtask ohos --release` 在 `target/ohos/release` 中生成相同结构。
声明文件在构建时从 Rust API 自动生成。

## API 基准与可选增强

默认 SDK 使用 API 20。按需选择以下 Cargo feature：

| 构建 | 原生能力 |
|---|---|
| 默认 | 系统编辑器、安全多行编辑、checked 语义、live region 播报、有序前景/背景滤镜、内容透明度阴影和 plus-darker 混合 |
| `api-23` | 自定义语义叶节点的无障碍 provider、增减动作、范围信息和系统减少动态效果设置观察 |
| `api-26` | 包含 `api-23`，并启用每节点最多 16 个具名自定义无障碍动作 |

```sh
cargo xtask ohos --features api-23
cargo xtask ohos --dev --features api-26
```

对应目录为 `target/ohos/debug-api-23` 和 `target/ohos/debug-dev-api-26`。
选择 `api-23` 或 `api-26` 会将最低运行版本提升到 API 23 或 API 26。
示例 manifest 遵循 SDK metadata，能力查询同时报告编译 feature 和实际系统版本。
宿主通过生成的 API 查询：

```ts
import { getCapabilities } from 'libnsh_platform_ohos.so';
const capabilities = getCapabilities();
// apiLevel、minimumApiLevel、compiledApi23、compiledApi26、api23、api26
```

配套的 `@nsh/ohos` HAR 持有 SDK 控件和纹理 producer，Native Shell 负责语义策略、
合成和原生视频播放。各 feature 配置使用相同的 `NshSurface` 和
`ContentSlot` 接入方式。

在动态播报节点上设置 `ohos={{ accessibilityLiveRegion: "polite" }}` 或 `"assertive"`。
嵌入示例提供独立的 OpenHarmony QEMU 验收脚本，验证系统焦点、动作和播报事件，
详见[测试说明](https://github.com/richerfu/nsh/tree/main/examples/embed-ohos#accessibility-acceptance-on-qemu)。

## 加入 DevEco 项目

在应用的 `entry` 模块中使用以下目录结构：

```text
entry/
  libs/arm64-v8a/
    libnsh_platform_ohos.so
    libc++_shared.so
  src/main/cpp/types/libnsh_platform_ohos/
    Index.d.ts
    oh-package.json5
  src/main/resources/rawfile/bundle.js
```

将 SDK 的 `index.d.ts` 复制为 `Index.d.ts`，并将
`$OHOS_NDK_HOME/native/llvm/lib/aarch64-linux-ohos/` 中的 `libc++_shared.so`
放在 NSH 原生库旁边。原生模块的 `oh-package.json5` 内容如下：

```json
{
  "name": "libnsh_platform_ohos.so",
  "version": "0.1.1",
  "types": "./Index.d.ts"
}
```

在 `entry/oh-package.json5` 中添加模块依赖，然后执行 OHPM install：

```json
{
  "dependencies": {
    "libnsh_platform_ohos.so": "file:./src/main/cpp/types/libnsh_platform_ohos"
  }
}
```

## 随应用附带 bundle

将 NSH 模块构建到应用的 rawfile 资源目录：

```sh
cargo build -p nsh-cli
./target/debug/nsh build examples/counter --release --out examples/embed-ohos/entry/src/main/resources/rawfile
```

调用 `surface.loadResource("bundle.js")` 后，NSH 在原生 worker 中通过
Ability 资源插件读取源码。导入的图片与字体从 `rawfile/assets/` 加载。资源随宿主签名 HAP 一起发布。源码使用运行时的常规加固策略和默认 VM 限制。

## 加载签名包

宿主在自己的私有文件目录中持有信任存储：

```ts
import { NshTrust } from 'libnsh_platform_ohos.so';

const trust = await NshTrust.open(stateDirectory, [rootPublicKeyBase64]);
const app = await trust.openPackage(downloadedPackagePath);
surface.runPackage(app);
```

`openPackage` 在线程池中校验签名、证书、能力声明、撤销和防回滚状态，并提取独立资源目录。
`openResource(name)` 对 HAP rawfile 中的签名包执行相同校验。`app.info` 包含
`id`、`name`、`version` 和精确十进制字符串 `build`。一个 surface 接收后，
`app.isConsumed` 变为 true；其他 surface 可重新打开该包。宿主授权与 VM 限制对每次运行生效。

`trust.addRoot(base64)` 添加宿主根公钥；`await trust.updateRevocations(json)` 校验并持久化
较新的签名撤销列表，返回精确十进制序列号。共享同一私有目录的存储对象串行更新，
并读取彼此已接受的构建号和撤销状态。校验失败保留正在运行的模块。

## 开发连接

`cargo xtask ohos --dev` 将开发 SDK 写入 `target/ohos/debug-dev`；加 `--release`
则写入 `target/ohos/release-dev`。运行 `nsh dev path/to/app`，再调用
`surface.connectDevServer('ws://host:8790/ws')`。每个视图独立持有连接，自动重连，
接收 source map、热更新和失败后的完整包恢复。原生和 JS 日志发送到开发终端，
构建与运行时错误显示在发起连接的 surface 上。错误面板在可滚动的完整错误正文上方
固定显示 Copy 和 Dismiss 按钮，支持阅读多行调用栈。Copy 将完整错误写入剪贴板，
写入成功后显示 Copied；写入失败时显示反馈，并保留原错误。Dismiss 隐藏面板，
保留 `lastError`；新错误会重新打开面板，接收替换源码时清除之前的错误。
每个 surface 独立管理自己的错误面板，首个模块启动前的构建失败也可查看和复制。
热更新遵循[共享状态保留规则](../../../docs/dev/tooling.md)。
`disconnectDevServer()` 保留当前模块；显式加载和 `stop()` 结束该视图的开发连接。

## 显示模块

在 DevEco 应用中使用 `@nsh/ohos`。应用的 `EntryAbility` 保持标准的 HarmonyOS
`UIAbility`（`extends UIAbility`）。在普通宿主字段中持有 `NshSurface` 实例，
加载资源，并将 `surface.content` 放入具有确定尺寸的 `ContentSlot` 容器：

```ts
import { NshSurface } from '@nsh/ohos';

@Entry
@Component
struct Index {
  private surface: NshSurface = new NshSurface();
  @State error: string = '';

  aboutToAppear(): void {
    this.surface.setErrorHandler((message: string) => { this.error = message; });
    this.surface.loadResource('bundle.js').catch((error: Error) => {
      this.error = error.message;
    });
  }

  aboutToDisappear(): void {
    this.surface.dispose();
  }

  build() {
    Column() {
      Text(this.error)
      Stack() { ContentSlot(this.surface.content) }
        .width('100%').layoutWeight(1)
    }.width('100%').height('100%')
  }
}
```

`@nsh/ohos` 在创建 surface 时自动关联当前 `UIContext`、`UIAbilityContext` 和窗口生命周期。
Rust 持有 `NodeContent`，测量原生容器尺寸并更新对应引擎的 viewport。
挂载前可以加载源码；引擎会在 slot 已挂载且尺寸为正时启动。

剪贴板读取遵循宿主应用的 `ohos.permission.READ_PASTEBOARD` 声明、运行时授权
和签名 profile；文本存在性查询仅检查 MIME 类型。

## 多个独立视图

业务通过对象引用区分视图，每个实例都有自己的模块、配置和原生组件工厂：

```ts
private primary: NshSurface = new NshSurface();
private secondary: NshSurface = new NshSurface({ memoryBytes: 67108864 });

// 在页面的 build 方法中：
Row() {
  Stack() { ContentSlot(this.primary.content) }.width('35%').height('100%')
  Stack() { ContentSlot(this.secondary.content) }.width('65%').height('100%')
}.width('100%').height('100%')
```

分别调用 `loadResource` 或 `loadSource` 加载模块，通过普通 ArkUI 容器定位、
调整大小和顺序。同一实例同一时间对应一个 ContentSlot；同一 bundle 可以在多个视图中独立运行。

移除 ContentSlot 时，只要保留实例，引擎和模块状态就会保留；重新挂载会复用原生树并适应当前尺寸。
`stop()` 取消待完成的加载并清空模块，保留视图、配置和工厂，随后可以再次加载。
`dispose()` 永久释放实例；新的生命周期使用新实例。Ability 销毁会释放所有剩余实例。
如果页面在 `aboutToDisappear` 中释放了实例，又保留并复用该页面对象，重新显示时需要创建新实例。

## 嵌入 API

所有 SDK 对象在 ArkUI 线程创建和使用，声明从 Rust 自动生成。
除 `initLogging` 外，下表成员均属于对应实例。

| 成员 | 说明 |
|---|---|
| `new NshSurface(options?: NshOptions)` | 校验限制和授权后创建空的原生视图。 |
| `readonly content: NodeContent` | 放入一个标准 ContentSlot 的稳定平台对象。 |
| `activate(): void` | 选择该 surface 接收系统返回操作，例如切换宿主 tab 时调用。 |
| `runPackage(app: NshPackage): void` | 消费已验证签名包，使用宿主授权和限制运行。 |
| `connectDevServer(address: string): void` | 将开发 SDK 连接到 WebSocket 开发服务器。 |
| `disconnectDevServer(): void` | 断开开发连接，保留当前模块。 |
| `loadSource(source: string, sourceMap?: string): void` | 接收可信源码并替换该实例的模块。 |
| `loadResource(name: string, sourceMap?: string): Promise<void>` | 异步读取相对路径的源码或预编译 HAP rawfile 及同目录能力声明，最多 64 MiB，并交给该实例。 |
| `stop(): void` | 取消待完成的加载并停止模块，视图可再次加载。 |
| `dispose(): void` | 永久释放引擎、原生树和工厂，可重复调用。 |
| `readonly isLoading: boolean` | 正在读取源码，或等待 slot 挂载且尺寸为正。 |
| `readonly isRunning: boolean` | 当前引擎已启动。 |
| `readonly isDisposed: boolean` | 实例已永久释放。 |
| `readonly lastError: string` | 最近的加载、引擎或挂载错误，接收新源码时清空。 |
| `setErrorHandler(handler: (message: string) => void): void` | 在 UI 线程接收异步加载、引擎和挂载错误。 |
| `setEnvironment(changes: NshEnvironment): void` | 更新该实例提供的环境字段，首次加载前也可调用。 |
| `registerComponent(name: string, factory: (id: number) => object): void` | 注册该实例的原生组件工厂。 |
| `unregisterComponent(name: string): void` | 移除工厂，已挂载组件继续持有自己的宿主对象。 |
| `emitComponentEvent(handle: number, name: string, data: string \| number \| boolean \| object \| null): void` | 将存活原生组件的事件发送给该实例的模块。 |
| `setComponentSize(handle: number, width: number, height: number): void` | 以 vp 报告存活原生组件的固有尺寸。 |
| `initLogging(): void` | 安装 tag 为 `nsh` 的可选 HiLog logger。 |

`loadResource` 的 Promise 在源码读取并被接收时完成；引擎初始化和 JS 执行随后异步进行，
相关错误通过 `lastError` 与错误回调观察。`isRunning` 描述引擎生命周期。
资源读取失败会保留原模块；新源码一旦被接收就会替换旧引擎，随后执行失败不会恢复旧模块。
最后一次加载生效：后续加载、`stop`、`dispose` 或 Ability 销毁都会使旧读取结果失效。
业务需要处理 Promise rejection，包括取消。每个会话最多允许 32 个同时进行的资源读取。

`NshOptions` 包含 `assetDirectory`、初始 `environment`、`capabilities`（宿主授权的 JSON schema）、
`memoryBytes`、`stackBytes`、`maxNodes`、`maxOpValues`、`maxCanvasOps`、`callBudgetMs` 和 `loadBudgetMs`。
构造时先校验选项再分配原生对象。指定资源目录后，图片和字体读取限定于该目录；
默认通过 HAP 资源插件加载。替换模块保留选项和组件工厂，source map 随每次加载传入。
原生 viewport 最大为 16,384 vp，会话像素比例范围为 `[0.25, 8]`。

`NshEnvironment` 支持安全区（`safeTop/Right/Bottom/Left`）、键盘高度和动画时长、字体缩放、
配色（`light`/`dark`）、locale、RTL、减少动态效果、读屏状态、设备类型（`phone`/`pad`/`desktop`/`tv`）
及 API level。安全区和键盘高度以相对于 surface 的 vp 表示。省略字段保持原值。
资源配置提供初始 locale、配色和设备类型；Ability 配置、accessibility 插件和窗口事件
更新字体缩放、读屏状态与安全区，宿主可通过 options 提供其他初始观测。
在 API 23+ 系统启用 `api-23` 时，无障碍插件还会自动观察减少动态效果设置。
输入框的无障碍标签和提示直接交给原生 TextInput/TextArea，显式焦点请求也定位实际编辑器。

Stack 支持滑入、淡入和无动画转场，以及从起始侧边缘发起的交互返回，遵循已挂载的
返回策略、手势开关和 RTL 方向。HTTP 与图片 worker 使用系统代理及排除规则；同一
surface 内相同图片共享加载工作，各消费者仍独立取消。

原生组件工厂接收实例内有效的 handle，返回持有 `node: ComponentContent` 的对象，
接收 `updateProps(NshNativeProperty[])`、`command(name, args)` 和 `dispose()` 回调。
使用组件的模块加载前注册工厂，已停止或替换引擎的旧 handle 会被拒绝。
该示例可选的宿主组件实现见 [HostBadge](../../../examples/embed-ohos/entry/src/main/ets/components/HostBadge.ets)。

构造和显式操作失败会通过 N-API 抛出异常。在同步原生组件回调中请求替换模块或 `stop()` 时，
应安排到下一次宿主任务执行；回调中的 `dispose()` 会将原生回收延后到该回调返回。
原生测量、布局和绘制回调失败记录日志。

## 运行限制

每个 surface 使用共享 Rust 运行时的默认限制：

| 限制 | 默认值 |
|---|---|
| JS 堆 | 256 MiB |
| 解释器栈 | 4 MiB |
| bundle 求值 | 10 秒 |
| 一次 JS 事件、定时器或帧调用 | 2 秒 |
| core 节点数 | 200,000 |

OHOS 文本渲染器按照原生栅格预算调整长文本的渲染比例，同时保留完整的逻辑尺寸。
原生位图内存与 JS 堆分开计算。运行时检查见[沙箱与能力](./sandbox-and-capabilities.md)，嵌入配置范围见
[后端参考](../../../docs/dev/ohos.md#implementation-status)。

## 平台行为

`Platform.OS` 为 `"ohos"`；`Platform.select` 依次查找 `ohos`、`mobile`、`native` 和
`default`。使用 `ohos:` 编写平台相关的类：

```tsx
import { Text, View } from "nsh";
import { Platform } from "nsh/device";

<View class="p-4 ohos:bg-zinc-100">
  <Text>{Platform.select({ ohos: "OHOS", default: "Native Shell" })}</Text>
</View>
```

后端挂载 View、Text、Input、Scroll、Image、原生组件和 Modal 节点，渲染
flex/grid 布局、纯色背景与边框、圆角、不透明度、裁剪、可见性和原生按压事件。
文本使用系统字体，支持字体族、字号、字重、斜体、颜色、对齐、行高和字距。响应式更新、
`If`/`Each`、定时器和 `requestAnimationFrame` 由共享 Rust 引擎驱动。
文本中的 NUL 字符显示为 U+FFFD。

`drop-shadow()` 根据原生子树的实时内容透明度生成阴影，支持嵌套滤镜和溢出子节点。
前景、背景滤镜按声明顺序执行，背景滤镜采样当前 Surface 背景根中先前绘制的内容。
`mixBlendMode: "plus-darker"` 按预乘颜色与透明度合成。合成期间，原生控件继续处理
输入和滚动。

`Input` 使用系统 TextInput 或 TextArea，焦点、选区、自动填充和无障碍由原生编辑器管理。
安全输入使用系统密码 TextInput，也适用于同时设置 `multiline` 的请求。每个编辑器独立
持有值、焦点、选区和生命周期。软键盘字符输入及退格会触发 `onKeyPress`。

Scroll 和 List 按圆角裁剪视口，仅在提供 `onRefresh` 或 `refreshing` 为 true 时创建下拉刷新容器。
滚动视图使用原生惯性与吸附，支持 `decelerationRate` 和
`disableIntervalMomentum`。缩放支持比例与偏移的动画更新、捏合中心保持、
有边界的缩放回弹和 `centerContent`。触摸坐标随显示变换和滚动缩放更新。
`keyboardShouldPersistTaps` 决定点击是否关闭输入框或交给事件处理器；
`keyboardDismissMode` 在拖动开始时关闭键盘。文本装饰支持指定粗细、下划线偏移，
以及实线、双线、点线、虚线和波浪线。

选择柄保持在滚动区域边缘时，原生帧时钟持续滚动内容并扩展选区，松手后停止。

`Text selectable` 支持按词选择、拖动范围手柄，以及由 NSH 自有 `@nsh/ohos` HAR 提供的
原生 Copy/Select All 菜单。`selectionColor` 设置高亮和手柄颜色。复制保留 Unicode
文字并跳过行内附件占位符。每个 Surface 独立持有选区，输入框获得焦点、文本替换和
点击其他位置时清除。Ctrl+C、Ctrl+A 和 Esc 操作当前选区，选区位置使用引擎线程的
原始文本排版。

## 构建并运行示例

在仓库根目录构建 SDK 和 CLI 后执行：

```sh
examples/embed-ohos/build.sh
# cargo xtask ohos --release 后使用 target/ohos/release：
NSH_PROFILE=release examples/embed-ohos/build.sh
```

脚本复制原生库和自动生成的声明，打包 Counter、安装 ArkTS 依赖，并用 Hvigor 构建
HAP。在 DevEco 中打开 `examples/embed-ohos`，为 `com.example.nsh` 配置开发签名。
将签名 HAP 安装到明确选定的设备：

```sh
hdc list targets
hdc -t <target> install -r <signed.hap>
hdc -t <target> shell aa start -a EntryAbility -b com.example.nsh
hdc -t <target> shell 'hilog -x -T nsh'
```

工具栏验证独立视图、宿主布局、保留状态的 **Unmount / Mount**、
**Stop / Reload**，以及实例释放和重建。
Counter 覆盖状态、点击分发、文字变色和历史行增删。修改模块后重新打包源码资源和 HAP；
修改原生实现后重新构建 SDK。

## 排查问题

| 现象 | 检查内容 |
|---|---|
| Cargo 无法获取锁定的原生源码 | 获取 `Cargo.lock` 中的确切 Git revision，见[原生依赖](../../../docs/dev/ohos.md#locked-native-dependencies)。 |
| ArkTS 无法导入原生模块 | 对齐 `.so` 名称、模块依赖和生成的 `Index.d.ts`，执行 OHPM install。 |
| 原生库加载失败 | 附带两个 arm64 库，并在 arm64 设备上运行 HAP。 |
| surface 显示为空 | 检查源码已就绪、容器尺寸非零，并读取 实例的 `isLoading`、`isRunning`、`lastError`和 HiLog。 |
| 模块请求了其他原生元素或操作 | 按模块使用的原语和命令核对[实现状态](../../../docs/dev/ohos.md#implementation-status)。 |

## 相关文档

- [平台概览](./platforms.md)
- [嵌入 iOS 应用](./ios.md)
- [嵌入 Android 应用](./android.md)
- [后端架构、实现状态与验证](../../../docs/dev/ohos.md)

## 原生 WebView 和视频

WebView 和视频各自属于 NshSurface 的原生子树。Ability WebView 插件持有
ArkWeb 控制器和 BuilderNode，NSH 保留原生挂载 owner，直到 SDK 确认退休完成。
NSH 视频通过 binding 持有 C XComponent、原生控制条和 AVPlayer worker。
UIContext 重建后按当前 props 恢复组件。宿主注册 WebView factory，
并展开 `@nsh/ohos` 的 `nativeShellPlugins()`。

WebView 支持内联 HTML、获准的 URL、独立的 `asset:/` 资源、文档初始化脚本、
JS 注入、双向消息、历史导航和重新加载。更新 JavaScript 开关、初始化脚本或 user agent
会保留控制器和导航历史；配置在下一次导航前完成。Video 支持远程 URL 与本地资源、暂停、
静音和音量、循环、缩放模式、原生控制条、seek 和播放事件。导入 `.mp4`、`.m4v`、
`.webm`、`.mov` 文件会得到打包后的资源 URL 字符串。在宿主授予完整网络权限时，
HTTP(S) 视频由 AVPlayer 流式播放，支持 HLS 和直播。受限 URL 通过磁盘缓存播放，
并检查重定向及系统代理配置。本地文件和文件系统资源直接使用文件描述符；HAP 资源
分块写入缓存。正数播放速率使用原生 0.125–4 范围，0 表示暂停。点击视频可切换控制条，
播放时三秒后自动隐藏；无关属性更新会保留用户手动选择的播放或暂停状态。