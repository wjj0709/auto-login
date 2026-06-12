# 站点-账户管理与数据查询(SQLite)设计文档

- 日期:2026-06-12
- 状态:已批准(头脑风暴结论)
- 范围:anyrouter-auto-login rust-version(GPUI 桌面应用)

## 1. 背景与目标

当前应用的账户与站点(Provider)配置全部来自环境变量(`ANYROUTER_ACCOUNTS` / `PROVIDERS`),无法在界面中管理;登录后产生的 cookie 留在 Playwright 临时浏览器上下文中即被丢弃;除余额外无法查看账户的其他站点数据。

本设计引入 SQLite 持久化与全新的卡片式界面,实现:

1. 站点、账户的界面化增删改查,按「站点 → 账户」两级组织;
2. Cookie 双来源:添加账户时手动录入,或账密登录自动获取(含真实过期时间解析)与自动续期;
3. 登录后只读查看账户数据:余额、使用统计、资源消耗、性能指标(日志耗时)、使用日志、模型消耗图表、API 密钥;
4. 全部界面文案使用中文。

## 2. 已确认的关键决策

| 决策点 | 结论 |
|---|---|
| 旧环境变量配置 | 完全替换:首次启动自动导入 SQLite,此后环境变量不再参与账户/站点配置 |
| 敏感信息存储 | 系统凭据库加密:keyring(Windows 凭据管理器)存主密钥,AES-256-GCM 加密列 |
| 站点数据能力 | 纯只读展示,不提供 API 密钥管理等写操作 |
| 数据获取策略 | 缓存 + 手动刷新:查询结果写 SQLite,详情页先展示缓存与更新时间 |
| 导航结构 | 无侧边栏;主页站点卡片 → 点击弹出账户列表弹窗 → 点击账户进入详情页 |
| 日志展示 | 底部栏右侧图标按钮,点击开关底部抽屉 |
| 原仪表盘 | 取消独立页面,汇总统计并入主页顶部统计条 |
| 技术选型 | rusqlite(同步,后台线程执行)+ 扩展现有 Python Playwright 脚本为多任务模式 |

## 3. 总体架构

```
UI 层(GPUI):root_view(改) home_view(新) account_modal(新) account_detail(新) log_drawer(改)
    ↕ 读取状态 / 派发动作
状态层:app_state(改)— sites/accounts/缓存/当前视图/弹窗抽屉开关/运行状态
    ↓ 调用
服务层:service(扩)— 签到(全部/单站点/单账户)、登录取 Cookie、拉取详情;完成后写库+更新状态
    ↓ 读写 / 子进程
基础设施:storage(新,rusqlite) crypto(新,keyring+AES-GCM) playwright.py(扩,多任务)
```

模块变更清单:

| 模块 | 变更 | 说明 |
|---|---|---|
| `storage.rs` | 新增 | rusqlite 封装:建表迁移、四张表 CRUD、`.env` 一次性导入 |
| `crypto.rs` | 新增 | 主密钥管理(keyring)+ AES-256-GCM 加解密(blob = nonce ‖ 密文) |
| `home_view.rs` | 新增 | 统计条 + 站点卡片网格 + 新建站点卡片 |
| `account_modal.rs` | 新增 | 账户列表弹窗、站点表单、账户表单、删除确认弹窗 |
| `account_detail.rs` | 新增 | 账户详情页:概览 / API 密钥 / 使用日志 / 消耗图表 四标签 |
| `chart.rs` | 新增 | GPUI div 自绘按日柱状图 + 模型 Top 排行 |
| `root_view.rs` | 改造 | 去侧边栏;标题栏(品牌+一键签到)+ 视图切换 + 底部栏 + 抽屉/弹窗层 |
| `app_state.rs` | 改造 | 数据源改为 SQLite 实体;状态键从展示名改为 `account_id` |
| `log_panel.rs` | 改造 | 重命名 `log_drawer.rs`,改为底部抽屉形态 |
| `service.rs` | 扩展 | `run_checkin(scope)` / `run_login(account_id)` / `run_fetch_detail(account_id)` |
| `playwright.rs` | 扩展 | 协议结构:action 字段、cookies/tokens/logs/chart 回传字段 |
| `scripts/playwright_checkin.py` | 扩展 | 多任务模式 checkin / login / fetch_detail,统一回传 `context.cookies()` |
| `config.rs` | 缩减 | 删除环境变量加载逻辑,保留结构定义供导入使用 |
| `dashboard.rs` `account_panel.rs` | 删除 | 功能并入 home_view / account_modal |
| `balance.rs` `checkin.rs` `notify.rs` | 不动 | 通知链路本次不涉及,维持 dead_code 现状 |

新增依赖:`rusqlite`(bundled)、`keyring`、`aes-gcm`、`rand`、`dirs`。

## 4. 数据模型

数据库文件:`dirs::data_local_dir()/anyrouter-checkin/data.db`(Windows 即 `%LOCALAPPDATA%\anyrouter-checkin\data.db`),目录不存在则创建。

```sql
CREATE TABLE sites (
  id             INTEGER PRIMARY KEY AUTOINCREMENT,
  name           TEXT NOT NULL UNIQUE,          -- 展示名,如 AnyRouter
  domain         TEXT NOT NULL,                 -- https://anyrouter.top
  login_path     TEXT NOT NULL DEFAULT '/login',
  sign_in_path   TEXT,                          -- NULL = 自动签到型站点
  user_info_path TEXT NOT NULL DEFAULT '/api/user/self',
  tokens_path    TEXT NOT NULL DEFAULT '/api/token/',
  logs_path      TEXT NOT NULL DEFAULT '/api/log/self',
  chart_path     TEXT NOT NULL DEFAULT '/api/data/self',
  api_user_key   TEXT NOT NULL DEFAULT 'new-api-user',
  created_at     TEXT NOT NULL,                 -- ISO8601 本地时间
  updated_at     TEXT NOT NULL
);

CREATE TABLE accounts (
  id                INTEGER PRIMARY KEY AUTOINCREMENT,
  site_id           INTEGER NOT NULL REFERENCES sites(id) ON DELETE CASCADE,
  name              TEXT NOT NULL,
  api_user          TEXT NOT NULL,              -- new-api-user 请求头的值
  username_enc      BLOB,                       -- AES-GCM 加密,可空
  password_enc      BLOB,                       -- AES-GCM 加密,可空
  cookies_enc       BLOB,                       -- AES-GCM 加密的 cookie JSON,可空
  cookie_issued_at  TEXT,                       -- Cookie 获取时间(登录/录入/续期时刻)
  cookie_expires_at TEXT,                       -- 过期时间;会话期或未知为 NULL
  created_at        TEXT NOT NULL,
  updated_at        TEXT NOT NULL,
  UNIQUE(site_id, name)
);

CREATE TABLE account_cache (
  account_id  INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  kind        TEXT NOT NULL,                    -- overview | tokens | logs | chart
  payload     TEXT NOT NULL,                    -- JSON;kind=tokens 时为 base64(AES-GCM 密文)
  encrypted   INTEGER NOT NULL DEFAULT 0,       -- 1 = payload 已加密
  fetched_at  TEXT NOT NULL,
  PRIMARY KEY (account_id, kind)
);

CREATE TABLE meta (
  key   TEXT PRIMARY KEY,                       -- schema_version / env_imported
  value TEXT NOT NULL
);
```

约束与说明:

- 外键级联:删站点 → 删其账户 → 删其缓存;`PRAGMA foreign_keys = ON`。
- cookie JSON 结构:`[{name, value, domain, path, expires, httpOnly, secure}]`(与 Playwright `context.cookies()` 对齐);手动录入的 `k=v; ...` 串解析后以同结构存储(domain 取站点域名,expires 置 -1)。
- `account_cache.payload`:`kind=tokens` 含完整 API 密钥,属敏感凭据,加密存储;`overview`(已脱敏)、`logs`、`chart` 明文存储。
- 时间统一 ISO8601 字符串(本地时区,含偏移),便于直接展示与比较。

### 加密方案(crypto.rs)

- 首次启动生成 32 字节随机主密钥,存入系统凭据库(service = `anyrouter-checkin`,user = `master-key`,值为 base64)。
- 加密:AES-256-GCM,每值独立 12 字节随机 nonce,存储格式 `nonce ‖ ciphertext`(BLOB;文本场景再 base64)。
- 凭据库初始化失败:启动弹窗报错并退出,不降级明文。

### 首次导入(storage 启动逻辑)

`meta.env_imported` 不存在时执行一次:

1. 插入内置站点 AnyRouter(anyrouter.top,手动签到)与 AgentRouter(agentrouter.org,自动签到),同名已存在则跳过;
2. 解析 `PROVIDERS` 环境变量,逐项建站点(路径字段缺省用默认值);
3. 解析 `ANYROUTER_ACCOUNTS`,按 `provider` 名挂到对应站点;cookies 内偷渡的 `_username`/`_password` 迁移到加密列;引用了不存在站点的账户跳过并记日志;
4. 写 `meta.env_imported = 1`。JSON 解析失败:日志警告、跳过导入、以空库启动。
5. `dotenvy` 保留,仅用于运维变量(`PYTHON_BIN`、`PLAYWRIGHT_SCRIPT`、`PLAYWRIGHT_HEADLESS` 等)。

## 5. 抓取层协议(Rust ↔ Python)

stdin 入参在现有基础上增加 `action` 与账户级路径字段:

```json
{
  "action": "checkin | login | fetch_detail",
  "headless": true,
  "timeout_ms": 30000,
  "accounts": [{
    "name": "...", "provider": "...", "domain": "...",
    "login_path": "...", "sign_in_path": null, "user_info_path": "...",
    "tokens_path": "/api/token/", "logs_path": "/api/log/self", "chart_path": "/api/data/self",
    "api_user_key": "new-api-user", "api_user": "...",
    "cookies": [/* 结构化 cookie 数组,或兼容旧 dict/串 */],
    "username": null, "password": null
  }]
}
```

stdout 出参,每账户一项:

| action | 字段 |
|---|---|
| `checkin` | `success / before / after / user_info / error / used_login` + **`cookies[]`(新增)** |
| `login` | `success / user_info / error` + `cookies[]` |
| `fetch_detail` | `success / user_info / tokens[] / logs[] / chart[] / error` + `cookies[]` |

- `cookies[]` 为任务结束时 `context.cookies()` 中属于站点域名的全部 cookie,元素含 `expires`(Unix 秒;**-1 表示会话期 cookie**)。
- `fetch_detail` 默认取数参数:tokens `?p=1&size=100`;logs `?p=1&page_size=50&type=0`;chart `?start_timestamp=now-7d&end_timestamp=now&default_time=day`。兼容分页参数差异由 Python 端按 new-api 约定拼接。
- 浏览器内 `fetch` 走页面上下文(现状不变),WAF 通过逻辑完全复用。

## 6. Cookie 生命周期

来源(统一加密落库 `accounts.cookies_enc` + 两个时间字段):

| 来源 | issued_at | expires_at |
|---|---|---|
| 手动录入 | 录入时刻 | NULL → 界面显示「有效期未知」 |
| 账密登录(`login` 任务) | 登录时刻 | session cookie 的 `expires`;-1 → NULL,标注「会话期」 |
| 任务顺带续期 | 回传 cookie 与库中不同时更新为当前时刻 | 同步取新值 |

任务执行前检查(checkin / fetch_detail):

- Cookie 有效 → 直接执行;
- 过期或缺失,**有账密** → 自动先 `login`(同一浏览器会话内),成功后继续原任务,用户无感;
- 过期或缺失,**无账密** → 任务中止,账户标 ⚠「Cookie 已过期,请更新」,日志记录原因。

界面 Cookie 状态色:有效(绿)/ 24 小时内到期(黄)/ 已过期(红)/ 有效期未知、会话期(灰)。

## 7. UI 设计

### 导航与视图

```rust
enum AppView { Home, AccountDetail(i64) }       // 整页切换,详情页面包屑返回
// 弹窗与抽屉(互斥性:同一时刻至多一个弹窗;抽屉独立开关)
account_list_modal: Option<i64 /*site_id*/>,
site_form: Option<SiteFormState>,                // 新建/编辑共用
account_form: Option<AccountFormState>,
confirm_delete: Option<DeleteTarget>,            // Site(id) | Account(id)
log_drawer_open: bool,
```

### 界面清单

1. **主页**:顶部统计条(站点数 / 账户数 / 总余额 / 今日签到 成功÷总数)+ 站点卡片网格(站点名、域名、账户数、Cookie 异常计数、余额合计、「查看账户」「✎」「🗑」)+「+ 新建站点」虚线卡片。
   - 余额类数字(统计条总余额、卡片余额合计)来自各账户 `account_cache.overview` 缓存求和;无缓存的账户不计入,数字旁以灰字标注「N 个账户未拉取」(存在时)。
   - 「今日签到」为本次运行会话内的签到成功/总数计数(沿用现有 success/fail 计数),不持久化。
2. **账户列表弹窗**:站点名标题;每行 = 账户名、余额、Cookie 状态、「详情 / ✎ / 🗑」;底部「+ 新增账户」「⚡ 签到本站点」。
3. **站点表单**:名称、域名必填(http(s):// 校验);高级设置默认收起(六个路径 + 请求头名,带 new-api 默认值;sign_in_path 留空 = 自动签到型)。
4. **账户表单**:名称、API 用户标识必填;凭据两种方式至少其一——粘贴 Cookie(`k=v;` 串或 JSON)/ 用户名+密码;按钮「保存」「登录获取 Cookie」(填账密后可用:先保存,再触发 login,成功回填 Cookie 与有效期,失败弹窗内显示错误)。
5. **账户详情页**:面包屑「◀ 返回主页 / 账户名 @ 站点名」+ 上次更新时间;顶部「⚡ 签到」「↻ 刷新数据」;四标签:
   - 概览:余额、累计消耗、请求数、用户信息摘要、Cookie 卡片(生效/过期时间 +「账密登录刷新」);
   - API 密钥:名称、key(默认打码 `sk-ab…yz`,点击复制完整值)、额度/已用、状态、过期时间;
   - 使用日志:最近 50 条——时间、模型、输入/输出 tokens、消耗额度、耗时(性能指标);
   - 消耗图表:近 7 天按日消耗柱状图(div 自绘,柱顶数值+日期标签)+ 模型消耗 Top 排行(请求数 / tokens / 金额)。
6. **底部栏**:左侧状态点(空闲绿 / 运行橙脉冲)+ 运行文案 + 成功/失败计数;右侧「▤ 日志」图标按钮(运行中带脉冲提示)开关底部抽屉。
7. **删除确认**:站点删除提示「将同时删除其下 N 个账户及全部缓存数据,此操作不可恢复」;账户删除同理。

签到入口:标题栏「一键签到全部」/ 账户弹窗「签到本站点」/ 详情页「签到」,对应 `CheckinScope::All | Site(id) | Account(id)`。

全部界面文案使用中文(含现有英文文案的替换)。

### 数据流(详情页)

打开详情 → storage 读四类缓存 → 立即渲染 +「上次更新 HH:MM」(无缓存显示「尚未拉取,点击刷新」)→ 用户点「刷新数据」→ `service::run_fetch_detail`(后台 tokio 任务:解密凭据 → 调 Python → 脱敏 → 写 account_cache + 续期 cookie → 更新 app_state)→ UI 自动刷新。期间按钮转加载态,页面可继续浏览旧数据。

storage 为同步 rusqlite,所有调用经 `tokio::task::spawn_blocking`(或服务层后台线程)执行,不阻塞 UI 线程;连接以 `Mutex<Connection>` 形式全局持有。

### 脱敏规则

- `overview` 存库前沿用现有 `sanitize_user_info` 规则(剔除 token/secret/password/cookie/session/auth/api_key 类字段);
- `tokens` 整体加密存储;UI 默认打码,显式操作才能复制完整 key;
- 日志输出沿用现有摘要脱敏(api_user 只显示前 8 位等)。

## 8. 错误处理

| 场景 | 处理 |
|---|---|
| 系统凭据库不可用 | 启动弹窗报错并退出,不降级明文 |
| 数据库打开/迁移失败 | 启动弹窗报错并退出 |
| `.env` 旧配置 JSON 损坏 | 日志警告,跳过导入,空库启动 |
| Python / Playwright 缺失 | 任务失败,日志给出安装指引(现状保留) |
| 登录失败(密码错/需验证码) | 表单/详情页内联显示错误,不落库无效 Cookie |
| 任务超时 / 网络失败 | 账户标失败,日志详情,既有缓存保留 |
| 表单校验失败 | 即时提示:必填项、域名格式、Cookie 解析失败、凭据二选一 |
| 站点接口结构不兼容 | 对应标签页显示「该站点不支持此数据」,其余标签不受影响 |

## 9. 测试策略

- `storage`::memory: 库覆盖建表迁移、四表 CRUD、级联删除、`.env` 导入幂等(重复启动不重复导入);
- `crypto`:加解密往返、密钥首次生成、损坏密文报错;
- 协议:Rust 端对回传 JSON 的反序列化容错(缺字段、`expires=-1`、空数组),cookie 过期判定边界(过去/未来/NULL);
- Python 脚本与 GPUI 界面无成熟自动化方案:每阶段附手动验收清单。

## 10. 分阶段交付

| 阶段 | 内容 | 验收 |
|---|---|---|
| 1 数据层 | storage + crypto + 迁移导入 | `cargo test`;导入后用 sqlite3 检查数据 |
| 2 UI 重构 | 主页卡片 / 账户弹窗 / 详情页骨架 / 日志抽屉;签到迁入新界面(全部/单站点/单账户) | 手动验收:增删改站点账户、签到流程、中文文案 |
| 3 登录取 Cookie | Python `login` 动作 + cookies 回传落库 + 过期自动重登/顺带续期 | 手动验收:账密登录回填、过期标注、无感续期 |
| 4 详情数据 | `fetch_detail` + 四类缓存 + 四标签页(含图表) | 手动验收:缓存秒开、刷新更新、打码复制 |

## 11. 不在本期范围

- API 密钥的创建/启停/删除等管理操作(已确认只读;界面预留扩展空间);
- 定时自动刷新详情数据;
- 通知链路(checkin.rs / notify.rs / balance.rs)的接入与改造;
- 多语言切换(固定中文)。
