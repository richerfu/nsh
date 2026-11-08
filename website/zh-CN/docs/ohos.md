---
title: OHOS
description: 通过 @nsh/ohos 和 NshSurface 将 NSH 应用嵌入现有 OHOS 应用。
order: 13
group: platforms
---

# OHOS

NSH 应用在 OHOS 上用 ArkUI 自定义节点和原生文本排版运行，要求 API 20 及以上，支持 `arm64-v8a`。现有应用可以在 `NshSurface` 中嵌入模块，加载 bundle 或签名包，并按自己设定的限制和能力运行它们。在 OHOS 上，`os()` 是 `Os::Ohos`（TSX 中 `platform.os` 是 `"ohos"`），TSX 的 `ohos={{…}}` 属性生效。

## 环境要求

- **DevEco Studio** 与 HarmonyOS / OpenHarmony SDK（API 20 或更高版本）、OHPM、Hvigor，以及 `arm64-v8a` 真机或模拟器。
- 在 macOS、Windows 或 Linux 上从源码构建 SDK 时需要 **Rust**（`rustup target add aarch64-unknown-linux-ohos`）。见[安装](./installation.md#构建-ohos-嵌入-sdk)。

## 嵌入 OHOS 应用

现有的 OHOS 应用可以在自己的界面中显示 NSH 应用。运行时以原生动态库（`libnsh_platform_ohos.so`）与配套 ArkTS 包（`@nsh/ohos`）的形式提供：创建一个 `NshSurface`，把 `surface.content` 放入标准的 `ContentSlot` 中，再往里加载一个 TSX bundle 或签名包即可。NSH 应用用原生 ArkUI 节点渲染，与你自己的组件并列。

每个 `NshSurface` 都拥有独立的引擎、JS VM 和 `NodeContent`，可以同时运行多个。窗口服务和有容量限制的资源线程池协调共享的系统资源。

<div class="diagram-scroll">
<img class="diagram diagram-light" src="/diagrams/host-app-light.svg" alt="一个宿主应用里有两个 nsh 视图：每个都是独立的引擎，有自己的线程、虚拟机、限制和能力；宿主授予能力、设置限制，并注册地图这样的原生组件。">
<img class="diagram diagram-dark" src="/diagrams/host-app-dark.svg" alt="一个宿主应用里有两个 nsh 视图：每个都是独立的引擎，有自己的线程、虚拟机、限制和能力；宿主授予能力、设置限制，并注册地图这样的原生组件。">
</div>

## 获取库

在 NSH 仓库根目录构建 SDK（`xtask` 会自动从 DevEco Studio 默认目录或 `OHOS_NDK_HOME` 查找 OpenHarmony NDK）：

```sh
cargo xtask ohos --release          # release：bundle 和签名包
cargo xtask ohos --release --dev    # 同时支持连接 `nsh dev` 开发服务器
```

这会在 `target/ohos/release/`（或 `target/ohos/release-dev/`）下生成 `arm64-v8a/libnsh_platform_ohos.so` 和自动生成的类型声明 `index.d.ts`，并在 `target/ohos/<profile>/arkts-deps/nsh/` 下暂存配套的 `@nsh/ohos` 包。面向 API 23 或 API 26+ 时，可按需传入 `--features api-23` 或 `--features api-26` 开启更高版本无障碍 provider 与减弱动态效果监听。

将 `libnsh_platform_ohos.so` 与 `$OHOS_NDK_HOME/native/llvm/lib/aarch64-linux-ohos/libc++_shared.so` 放入 `entry/libs/arm64-v8a/`，将 `index.d.ts` 复制到 `entry/src/main/cpp/types/libnsh_platform_ohos/Index.d.ts`，并在 `entry/oh-package.json5` 中引入：

```json5
{
  "dependencies": {
    "@nsh/ohos": "file:../ohos_nsh",
    "libnsh_platform_ohos.so": "file:./src/main/cpp/types/libnsh_platform_ohos"
  }
}
```

## 随应用附带 bundle

SDK 包含 `Editor`、`DiffView`、富文本编辑、图表、语法高亮和公式排版。`Shader` 通过 XComponent 绘制填充。
Shader 特效在 GPU 上处理原生内容纹理，并通过 XComponent Surface 呈现。内容、整屏、页面和弹窗转场由 Shader 进度驱动，特效显示期间原生控件仍可交互。
整屏特效作用于嵌入视图的根节点，弹窗在独立原生表面上使用自己的 `transitionShader`。

Shader 的 overflow 将捕获和输出区域扩展到视图逻辑边界之外。较大的目标按比例降低渲染分辨率。
特效可采样合成后的背景，与 CSS 遮罩和滤镜共同工作；导航转场在合成线程保留离场画面。

把 TSX 应用构建到模块的 `rawfile` 资源目录中：

```sh
nsh build App.tsx --release --out entry/src/main/resources/rawfile
```

这会写出 `bundle.nshc`（字节码）、`bundle.js`（源码）、`bundle.js.map`、`capabilities.json`（当 `nsh.json` 声明了[能力](./sandbox-and-capabilities.md)时）以及 `assets/`（应用导入的图片、字体和媒体资源）。

## 显示应用

```ts
import { NshSurface } from '@nsh/ohos';

@Entry
@Component
struct Index {
  private surface: NshSurface = new NshSurface(this.getUIContext());

  aboutToAppear(): void {
    this.surface.setErrorHandler((message: string) => {
      console.error(`nsh: ${message}`);
    });
    this.surface.loadResource('bundle.js');
  }

  aboutToDisappear(): void {
    this.surface.dispose();
  }

  onBackPress(): boolean {
    return this.surface.onBackPress();
  }

  build() {
    Stack() {
      ContentSlot(this.surface.content)
    }
    .width('100%')
    .height('100%')
  }
}
```

`NshSurface` 也可以放在任意 ArkUI 布局中，例如放在你自己的工具栏下方或并排放入 `Row`：

```ts
Column() {
  Toolbar()
  Stack() {
    ContentSlot(this.surface.content)
  }
  .width('100%')
  .layoutWeight(1)
}
.width('100%')
.height('100%')
```

插槽的尺寸就是应用的窗口：NSH 按插槽的尺寸布局应用，尺寸变化时重新布局。再次加载会替换正在运行的应用。完整示例见 `examples/embed-ohos/entry/src/main/ets/pages/Index.ets`。

## `NshSurface`

| 成员 | 说明 |
|---|---|
| `new NshSurface(uiContext: UIContext, options?: NshOptions)` | 创建一个绑定到 `uiContext` 的空视图，可配置 VM 限制、授予的能力、资源目录和初始环境。 |
| `readonly content: NodeContent` | 挂载到标准 `ContentSlot` 中的平台 `NodeContent` 对象。 |
| `setErrorHandler(handler: (message: string) => void)` | 在 UI 线程上接收应用的致命错误（加载失败、未捕获的 JS 异常、VM 因触及限制而停止）。错误总会写入日志。 |
| `setThemeOverrides(json?: string \| null)` | 把宿主的品牌加在应用的[主题](./tailwind.md#主题-token)之上：格式与 `nsh.json` 的 `"theme"` 相同的主题文件。立即生效，并作用于之后加载的应用；`null` 恢复应用自己的主题。无法解析时抛出异常。 |
| `setEnvironment(changes: NshEnvironment)` | 为该视图覆盖语言地区、字体缩放、配色方案（`"light"` / `"dark"`）、安全区域或键盘高度。未传字段保持原值。 |
| `loadResource(name: string, sourceMap?: string): Promise<void>` | 运行 `resources/rawfile/` 中的 `bundle.js` 或 `bundle.nshc`；应用资源取自 `rawfile/assets/`。 |
| `loadSource(source: string, sourceMap?: string)` | 直接运行可信 JS 源码并替换正在运行的应用。 |
| `runPackage(pkg: NshPackage)` | 运行一个已验证的签名包，替换正在运行的应用。每次 `openPackage` 打开的包只能运行一次。 |
| `registerComponent(name: string, factory: (id: number) => object)` | 为该视图注册一个[原生组件](./native-components.md)。 |
| `emitComponentEvent(handle: number, name: string, data: object \| string \| number \| boolean \| null)` | 从已挂载的原生组件向 JS 发送事件。 |
| `setComponentSize(handle: number, width: number, height: number)` | 以 `vp` 为单位上报已挂载原生组件的固有尺寸。 |
| `setParams(params?: object \| null)` | 之后加载的应用的启动参数：应用在启动时以 `host.params` 读取（[`nsh/host`](../modules/nsh-host.md)）。 |
| `send(name: string, data?: object \| string \| number \| boolean \| null)` | 向应用发送一个事件（`host.on(name, …)`）；应用开始监听前会一直保留，没有应用运行时留给下一个应用。 |
| `handle(name: string, handler?: ((data: object \| string \| number \| boolean \| null, reply: NshHostReply) => void) \| null)` | 在 UI 线程处理应用对 `name` 的调用（`host.call`）和投递（`host.post`）；传 `null`（或调用 `removeHandler(name)`）移除处理函数。通过 `reply.resolve(value)` 或 `reply.reject(message)` 回答一次。 |
| `setOpenUrlHandler(handler?: ((url: string) => boolean) \| null)` | 拦截应用打开的 URL；宿主自行处理时返回 `true`，交由系统打开时返回 `false`。 |
| `connectDevServer(url: string)` | 开发用：从 `nsh dev` 服务器加载，每次修改后重新加载（需使用 `--dev` SDK）。 |
| `disconnectDevServer()` | 断开开发服务器连接，保持当前应用继续运行。 |
| `activate()` | 在多个视图之间切换时，将该视图标记为系统返回手势的当前活动目标。 |
| `onBackPress(): boolean` | 将页面的 `onBackPress()` 事件路由到该视图所属的窗口会话；应用消费返回动作时返回 `true`。 |
| `stop()` | 停止正在运行的应用并取消未完成的加载，保留视图、配置和已注册的原生组件供后续加载使用。 |
| `dispose()` | 永久停止应用并释放该视图的原生资源。 |
| `setStorageDirectory(path?: string \| null)` | 为后续加载设置绝对存储目录；省略或传入 `null` 使用宿主的默认目录。 |
| `setCacheOptions(options?: NshCacheOptions \| null)` | 为后续加载设置磁盘缓存；省略或传入 `null` 恢复宿主默认值。 |
| `unregisterComponent(name: string)` | 移除之后挂载时使用的原生组件工厂，已经挂载的组件保留其所有者。 |
| `removeHandler(name: string)` | 移除 `handle` 注册的宿主处理函数。 |
| `readonly isRunning: boolean` | 当前模块的引擎正在运行。 |
| `readonly isDisposed: boolean` | 该视图已永久释放。 |
| `readonly isLoading: boolean` | 源码正在读取，或等待已挂载且尺寸有效的容器；随后异步开始执行。 |
| `readonly lastError: string` | 最近一次加载、引擎或原生挂载错误；接受新源码时清除。 |

## 与应用通信

视图与它运行的应用通过一条专属通道通信（[`nsh/host`](../modules/nsh-host.md)）：应用在启动时读取的启动参数、宿主应用发来的事件，以及应用发出的调用和投递，数据都是 JSON 值。应用里不需要任何视图，也不需要原生组件。

```ts
// 加载之前：应用首先打开的内容（TSX 中的 `host.params`）。
surface.setParams({ conversation: 'conv-42' });
surface.loadResource('bundle.js');

// 应用的 `await host.call("user.profile", { id })`，在 UI 线程处理。
surface.handle('user.profile', (data, reply) => {
  const req = data as Record<string, Object>;
  api.fetchProfile(req['id'] as string)
    .then((profile) => reply.resolve({ id: req['id'], name: profile.name }))
    .catch((err: Error) => reply.reject(err.message));
});

// 应用的 `host.post("analytics", …)`：没有回答。
surface.handle('analytics', (data, _reply) => {
  analytics.log(data);
});

// 之后：应用通过 `host.on("conversation.open", …)` 收到它。
surface.send('conversation.open', { id: tappedConversation });
```

处理函数在 UI 线程运行；调用在应用中等待回答，不会阻塞任何一方。应用尚未监听的名称的事件会保留到它开始监听，没有应用运行时发送的事件交给下一个应用。

## 主题覆盖

嵌入在多个品牌宿主应用中的同一个应用会采用各个宿主的品牌：宿主给出其品牌要改的 token，格式与 `nsh.json` 的 `"theme"` 相同的主题文件（shadcn/ui 的 `cssVars`，值按 CSS 的写法）。它们覆盖应用的同名 token，应用的其他 token 保持不变。在 `dark` 下给出 token 的深色模式值（只在 `light` 或 `theme` 下设置的 token 在两种模式下都是这个颜色）：

```ts
surface.setThemeOverrides(JSON.stringify({
  cssVars: {
    theme: { radius: '0.75rem' },
    light: { primary: '#e11d48', 'primary-foreground': '#ffffff' },
    dark: { primary: '#fb7185', 'primary-foreground': '#1c1917' },
  },
}));
surface.loadResource('bundle.js');
```

在加载之前或之后调用都可以：`bg-primary`、`rounded-(--radius)` 等类名立即更新样式，之后加载的每个应用在第一帧就带有这些覆盖。`setThemeOverrides(null)` 恢复应用自己的主题。

## 应用打开的链接

应用打开的 URL（[`openUrl`](../modules/nsh-device.md#openurl)，以及 `links="open"` 的 `TextView` 中的链接）先交给视图的 `setOpenUrlHandler` 回调，在 UI 线程调用：宿主应用自己处理了这个 URL（例如交给自己的路由）时返回 `true`，返回 `false` 则由系统打开。没有设置处理函数时，所有 URL 都由系统打开。到达这里的 URL 都已通过应用的 `openUrl` 能力检查。

```ts
surface.setOpenUrlHandler((url: string): boolean => {
  if (url.startsWith('lb://') || url.startsWith('https://longbridge.com/')) {
    router.open(url);
    return true;
  }
  return false;
});
```

## 限制

创建视图时通过 `NshOptions` 传入 VM 与节点限制：

```ts
const surface = new NshSurface(this.getUIContext(), {
  memoryBytes: 128 * 1024 * 1024, // 128 MiB 堆
  stackBytes: 4 * 1024 * 1024,    // 4 MiB 栈
  loadBudgetMs: 10_000,           // 加载 10 s
  callBudgetMs: 1_000,            // 每次调用 1 s
  maxNodes: 50_000,               // 50,000 个节点
});
```

默认值为：256 MiB 堆、4 MiB 栈、加载 10 s、每次调用 2 s、200,000 个节点。各项限制触发时会发生什么，见[沙箱与能力](./sandbox-and-capabilities.md)。

## `NshOptions`

所有字段均为可选项。

| 字段 | 默认值和用途 |
|---|---|
| `assetDirectory?: string` | 构建产物 `assets/` 的目录；默认从宿主 rawfile 读取。 |
| `storageDirectory?: string` | 后续加载使用的绝对存储目录；默认使用宿主 `filesDir`。 |
| `cacheOptions?: NshCacheOptions` | 磁盘缓存策略，默认使用宿主 `cacheDir`，每个应用的图片缓存上限为 256 MiB。 |
| `managesStatusBar?: boolean` | 默认 `true`；应用明确设置状态栏或进入全屏视频后才接管窗口栏。被动嵌入视图保留宿主的窗口设置。 |
| `environment?: NshEnvironment` | 初始系统环境覆盖。 |
| `capabilities?: string` | 宿主授予的能力，使用 `nsh.json` 的 JSON 格式。 |
| `memoryBytes?: number` | 默认 256 MiB，最小 1 MiB。 |
| `stackBytes?: number` | 默认 4 MiB，可设为 64 KiB 至 4 MiB。 |
| `maxNodes?: number` | 默认 200,000，最小 1。 |
| `maxOpValues?: number` | 默认 8,000,000 个待处理操作值，最小 1。 |
| `maxCanvasOps?: number` | 默认每次画布绘制 1,000,000 个操作，最小 1。 |
| `loadBudgetMs?: number` | 默认 10,000 ms，最小 1。 |
| `callBudgetMs?: number` | 默认 2,000 ms，最小 1。 |

`NshCacheOptions` 包含 `directory?: string`（绝对路径）、`disabled?: boolean`（默认 `false`）和
`imagesMaxMegabytes?: number`（图片缓存上限，`0` 关闭图片缓存）。`disabled: true` 与 `directory` 互斥。
`NshEnvironment` 包含 `pixelRatio`、`fontScale`、`colorScheme`、`locale`、`rtl`、`reduceMotion`、
`screenReader`、`idiom`、`lifecycle`、`apiLevel`、`safeTop`、`safeRight`、`safeBottom`、`safeLeft`、
`keyboardHeight` 和 `keyboardDuration`；长度使用 vp，时长使用 ms。

## 能力

通过 `NshOptions.capabilities` 传入宿主的授权（JSON 格式与 `nsh.json` 的 `capabilities` 相同）：

```ts
const surface = new NshSurface(this.getUIContext(), {
  capabilities: JSON.stringify({
    network: { hosts: ['api.example.com'] },
    webview: { hosts: ['help.example.com'] },
    host: true,
  }),
});
surface.loadResource('bundle.js');
```

一个请求必须既被应用在 `nsh.json` 中的声明允许，也被这里的授权允许。参见[沙箱与能力](./sandbox-and-capabilities.md)。

## 签名包

使用 `NshTrust` 在后台工作线程验证下载的包，再交给视图运行：

```ts
import { NshTrust } from '@nsh/ohos';

const trust = await NshTrust.open(`${context.filesDir}/nsh-trust`, [rootPublicKey]);
const pkg = await trust.openPackage(downloadedFilePath);
surface.runPackage(pkg);
```

| API | 说明 |
|---|---|
| `NshTrust.open(stateDirectory: string, rootPublicKeys: string[]): Promise<NshTrust>` | 打开或创建信任库。它的状态（最新的吊销列表、每个应用已接受的最高构建号）持久化在该目录中，包的资源也解压到其下。 |
| `addRoot(publicKeyBase64: string): Promise<void>` | 信任一个根公钥（base64，即 `nsh keygen` 生成的 `.pub` 文件内容）。 |
| `updateRevocations(revocationsJson: string): Promise<string>` | 应用 `nsh revoke` 生成的吊销列表；该列表须由受信任的根签名，且比信任库现有的更新。 |
| `openPackage(packagePath: string): Promise<NshPackage>` | 在工作线程验证包、解压资源并接受该包（此后该应用更旧的构建都会被拒绝）。 |
| `openResource(name: string, uiContext?: UIContext): Promise<NshPackage>` | 验证并打开打包在 `resources/rawfile/` 中的签名 `.nshpkg` 包。 |
| `NshPackage` | `info`（`id`、`name`、`version` 和十进制字符串 `build`）、`isConsumed`。 |

密钥、证书和吊销机制的工作方式见[签名包](./signed-packages.md)。

`openResource` 省略 `uiContext` 时使用唯一的活动 NSH 窗口会话；首次创建视图之前或多个窗口同时活动时，显式传入 `UIContext`。

## 模块导出

所有宿主 API 都从 `@nsh/ohos` 导入。

| 导出 | 用途 |
|---|---|
| `getCapabilities(): NshCapabilities` | 读取实际系统 API 级别与构建时启用的增强功能。 |
| `NshCapabilities` | `apiLevel`、`minimumApiLevel`、`compiledApi23`、`compiledApi26`、`api23`、`api26`。 |
| `initLogging(): void` | 将 Rust 日志接入 HiLog。 |
| `onBackPressIntercept(uiContext: UIContext): boolean` | 向指定窗口会话分发返回动作，返回是否已消费。 |
| `onNotificationWant(want: Want): void` | 从 Ability 的 `onCreate` 和 `onNewWant` 转发通知点击，包括冷启动。 |
| `NshHostReply` | `resolve(value)`、`reject(message)`、`isPost`，用于宿主通道的回答。 |
| `NshNativeProperty` | 原生组件批量属性更新中的 `name`、`value` 和 `removed`。 |

包还导出 `NshSurface`、`NshTrust`、`NshPackage`、`NshPackageInfo`、`NshOptions`、`NshEnvironment` 和 `NshCacheOptions`。

## 开发

使用 `--dev` SDK（`cargo xtask ohos --dev`）时：

```ts
surface.connectDevServer('ws://127.0.0.1:8790/ws');
```

在电脑上运行 `nsh dev App.tsx`。对于模拟器或 USB 连接的设备，使用 `hdc` 转发端口，让设备自己的 `127.0.0.1` 能访问到服务器：

```sh
hdc rport tcp:8790 tcp:8790
```

否则使用 `nsh dev` 打印的局域网地址。

## Ability 配置

- **标准 `UIAbility`。** 创建 `NshSurface` 时，`@nsh/ohos` 会自动关联当前的 `UIContext`、`UIAbilityContext` 和窗口生命周期，宿主的 `EntryAbility` 保持标准的 `UIAbility`（`extends UIAbility`），无需自定义基类。配套包会自动将 WebView、输入框、无障碍和原生组件片段挂载到各个视图中。
- **返回键。** 当应用需要处理返回键时（有打开的对话框、有 `BackHandler` 监听器、有被推入的导航页面），活动的 `NshSurface` 会优先接收返回手势，否则交给 Ability。同时展示多个视图（如多标签页）时，可在切换时调用 `surface.activate()`。
- **键盘与安全区域。** `@nsh/ohos` 自动观测系统栏、挖孔区域和软键盘避让区域，并只将与各个 `NshSurface` 重叠的部分作为 insets 上报。`Input` 使用系统 `TextInput` / `TextArea`，并自动滚动让获得焦点的输入框不被键盘遮挡。
- **生命周期。** 移除 `ContentSlot` 时，只要保留 `NshSurface` 实例，运行中的 VM 和状态就会保持；重新挂载会恢复原生节点树。调用 `surface.stop()` 可清空当前运行的应用并保留视图，页面永久销毁时在 `aboutToDisappear` 中调用 `surface.dispose()`。

原生输入框报告文本粘贴、选区、光标边界和输入法组合事件。只读输入框支持聚焦、选择和复制，
多行输入框通过原生滚动控制器响应 ref 的滚动操作。自动更正遵循系统键盘设置。

窗口缩放会刷新已打开的弹窗。宿主通过 `setEnvironment` 更新 `pixelRatio`，配置变化会刷新原生几何与文本密度。
Rust 宿主可在加载模块前通过 `nsh_platform_ohos::set_ui_sets(Vec<Registry>)` 注册额外的 UI 集。
以 `Nsh` 开头的原生组件名属于 SDK，宿主工厂使用自己的前缀。

## 另请参阅

- [用 Rust 开发](./rust.md)
- [嵌入 iOS 应用](./ios.md)
- [嵌入 Android 应用](./android.md)
- [原生组件](./native-components.md)
- [签名包](./signed-packages.md)
- [沙箱与能力](./sandbox-and-capabilities.md)

## Bundle 交付与宿主策略

`nsh build --split` 生成的 `app.json` 包含 `entryChunks`，为 `bundle.js` 和 `bundle.nshc` 预加载静态依赖。
将 chunk 文件及其配套文件复制到 rawfile 入口旁。单文件入口可省略 `app.json`，旧版源码入口会扫描静态导入。
预加载上限为 1024 个 chunk、总计 64 MiB，包含入口、配套文件和 source map。
`capabilities.json` 始终提供应用声明，即使宿主同时设置 `NshOptions.capabilities`，每次操作也要同时满足两者。

`NshTrust.openPackage` 和 `openResource` 的包大小上限为 256 MiB。`runPackage` 的同步预检查或接受失败后，
已验证包仍可重试；接受后包被消费一次，包括等待首次布局的应用。

OHOS 上 `nsh/mobile.isAvailable` 为 `true`，`TabBar` 使用 ArkUI，`BackHandler` 接收系统返回动作。
`process.platform` 和 `os.platform()` 返回 `"ohos"`。启动进程会返回 `ERR_FEATURE_UNAVAILABLE_ON_PLATFORM`。
嵌入宿主管理 bundle 和 HAP 更新，`installAndRestart` 使用同一错误码说明更新由宿主负责。

## 系统界面与媒体

导航栏、弹窗和页面的默认颜色优先使用应用的主题 token。导航返回控件使用矢量箭头和按钮无障碍角色，
标题在实际左右控件之间布局。弹窗操作的文字由应用提供，文字测量随系统字体缩放。
应用显式设置状态栏或播放全屏视频时，`managesStatusBar` 默认允许控制系统栏；设置为 `false` 由宿主管理。

`<WebView ohos={{ sharedCookies: false }}>` 使用 ArkWeb 私密 cookie 存储，并在首次导航前清空它；
默认共享 cookie。HTML 文件上传会打开系统照片或文档选择器，遵循单选或多选模式。
图片选择器在后台线程读取文件，图片与 SVG 使用系统代理和共享的缓存预算。每个视图可同时进行最多 32 个 fetch 请求，
持续响应流由独立读取线程处理。

API 20 通过 ArkTS 包装节点暴露选中和勾选状态。`api-23` 开启原生无障碍 provider、状态描述、
减弱动态效果监听以及动画图片逐帧时长；API 20 按总时长均分动画帧。`api-26` 增加具名自定义无障碍操作。
弹出控件可通过应用提供的 `activate` 操作标签描述其用途。视频音频遵循宿主的音频策略。
嵌入式 Ability 的退出由宿主管理；`BackHandler.exitApp()` 的请求记录在日志中。
