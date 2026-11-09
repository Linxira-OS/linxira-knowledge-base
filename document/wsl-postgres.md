# WSL 全局 PostgreSQL（linxira-2026-10-27）：搭建与使用

> 状态：生效中。本文件描述 Windows 主机 + WSL2 内全局 PostgreSQL 的部署、连接与复用方式。
> 适用对象：在本机开发、需要真实 PostgreSQL 的仓库（当前：lkb）。
> 安全边界：仅供本机（Windows 宿主 + WSL2 虚拟机）开发使用；无 TLS；未对局域网开放。

## 1. 部署概览

| 项 | 值 |
| --- | --- |
| WSL 发行版 | `linxira-2026-10-27`（Arch 系 “Linxira OS”，`systemd=true`） |
| 内核 | 6.18.33.2-microsoft-standard-WSL2 |
| PostgreSQL | **18.6**（pacman 包 `postgresql 18.6-4`；含 psql 客户端与 contrib 模块） |
| 数据目录 | `/var/lib/postgres/data`（`postgres:postgres`，0700） |
| 服务单元 | `postgresql.service`（enabled + active），监听 `0.0.0.0:5432` 与 `[::]:5432` |
| 角色 | `postgres`（超级用户；服务器本地 peer 登录，未设密码）、`lkb`（LOGIN，SCRAM-SHA-256） |
| 数据库 | `lkb`、`lkb_test`（owner=`lkb`；均已 `REVOKE CONNECT ... FROM PUBLIC`） |
| 工作区 | `/home/builder/workspaces/lkb`（脚本）、`/srv/lkb/backups`（备份） |

可复现的安装步骤（当时执行）：

```bash
# WSL 内，root
pacman -Syu --noconfirm postgresql          # Arch 系发行版；无 apt
runuser -u postgres -- initdb --encoding=UTF8 --locale=C.UTF-8 \
    --auth-local=peer --auth-host=scram-sha-256 -D /var/lib/postgres/data
systemctl enable --now postgresql
# 角色/数据库/环境变量：/home/builder/workspaces/lkb/lkb-pg-provision.sh
```

## 2. 服务启停与状态

```bash
# WSL 内（root）
systemctl status postgresql        # 详单
systemctl restart postgresql       # 重启
systemctl stop postgresql          # 停止
journalctl -u postgresql -n 50     # 日志（PG 未开 logging_collector，默认写 journald）
runuser -u postgres -- pg_isready  # 就绪探测
```

一键概览（版本 / 服务 / 监听 / 角色 / 库 / env 文件，不打印密码）：

```bash
/home/builder/workspaces/lkb/lkb-pg-status.sh   # 需 root
```

## 3. 连接方式（Windows → WSL）

WSL2 为 NAT 网络（本机为 `172.23.85.43/20`，网关 `172.23.80.1`）。已实测以下两种方式均可用：

| 方式 | 地址 | 说明 |
| --- | --- | --- |
| **localhost（当前默认）** | `localhost:5432` | WSL2 localhost 转发；跨 WSL 重启稳定，无需维护 IP |
| 直连 WSL 网卡 IP | `172.23.85.43:5432`（示例） | WSL 重启后 IP 会变化，见第 7 节 |

获取当前 WSL IP（该发行版的 `hostname` 来自 inetutils，**不支持 `-I`**）：

```bash
wsl -d linxira-2026-10-27 -e bash -lc "ip -4 -o addr show dev eth0 | awk '{print \$4}' | cut -d/ -f1"
```

WSL 内部也可直接 `psql "postgres://lkb:***@127.0.0.1:5432/lkb"`（用 env 文件里的 URL）。

### 配置摘要

`postgresql.conf`（托管段，原文件备份为 `postgresql.conf.lkb-orig`）：

```ini
listen_addresses = '*'
password_encryption = 'scram-sha-256'
```

`pg_hba.conf`（托管段，原文件备份为 `pg_hba.conf.lkb-orig`）：

```
# TYPE  DATABASE  USER      ADDRESS          METHOD
local   all       postgres                   peer
local   all       all                        scram-sha-256
host    all       all       127.0.0.1/32     scram-sha-256
host    all       all       ::1/128          scram-sha-256
host    all       all       172.16.0.0/12    scram-sha-256
```

安全边界说明：

- `listen_addresses='*'` 只影响 WSL 虚拟机内部；**未**做 `netsh portproxy` / 防火墙转发，局域网其他机器不可达；
- 所有 host 连接（含 Windows 宿主）必须通过 SCRAM-SHA-256 密码认证，来源限回环与 WSL NAT 段（`172.16.0.0/12`，覆盖 WSL 重启后可能的网段变化）；
- 未启用 TLS：仅适合本机回环/虚拟链路，不要经不可信网络连接。

## 4. 凭据与环境变量

环境变量文件（含密码；均限制权限，且不在 git 内）：

| 位置 | 权限/ACL | 说明 |
| --- | --- | --- |
| `/root/.config/lkb/pg.env` | 0600 | WSL root 用户 |
| `/home/builder/.config/lkb/pg.env` | 0600 | WSL builder 用户 |
| `F:\CODE\RAG\.lkb\pg.env` | NTFS ACL 仅 `BHYS\ETPau` | Windows 仓库侧（`.gitignore` 已忽略 `.lkb/`） |

文件格式（模板见 `document/pg.env.example`，**文档中不写真实密码**）：

```env
LKB_DATABASE_URL=postgres://lkb:REPLACE_WITH_PASSWORD@localhost:5432/lkb
LKB_TEST_PG_URL=postgres://lkb:REPLACE_WITH_PASSWORD@localhost:5432/lkb_test
```

Windows Git Bash 中使用：

```bash
set -a; source .lkb/pg.env; set +a
cargo +stable-x86_64-pc-windows-gnu test --test postgres_integration -- --nocapture
```

PowerShell 中使用：

```powershell
$env:LKB_TEST_PG_URL = ((Get-Content .lkb\pg.env | Where-Object { $_ -match '^LKB_TEST_PG_URL=' }) -replace '^LKB_TEST_PG_URL=', '')
```

禁止事项：把密码贴进聊天/issue/日志、提交 `pg.env`、在文档或代码中写真实密码。

## 5. 工作区脚本（`/home/builder/workspaces/lkb/`）

| 脚本 | 用途 |
| --- | --- |
| `lkb-pg-provision.sh` | 幂等重放：initdb、托管配置、服务、角色/库、env 生成。密码来源=`/root/.config/lkb/pg.env`；删掉该文件后重跑=轮换密码 |
| `lkb-pg-env-refresh.sh [auto\|localhost\|<host>]` | 按当前 WSL IP（或指定 host）重写三处 `pg.env`；不打印密码 |
| `lkb-pg-status.sh` | 版本/服务/监听/角色/库/env 状态一览 |
| `lkb-pg-dump.sh` | `pg_dump` lkb + lkb_test 到 `/srv/lkb/backups`（custom 格式） |

## 6. 其他项目如何复用（一个实例，多项目）

架构：**一个全局 PG 实例**，每个项目独立的 role + database，互不影响。

```sql
-- WSL 内以超级用户执行：
sudo -u postgres psql
CREATE ROLE myproj LOGIN PASSWORD '自行生成强随机密码';
CREATE DATABASE myproj OWNER myproj;
REVOKE CONNECT ON DATABASE myproj FROM PUBLIC;
```

- `pg_hba.conf` 已有回环与 `172.16.0.0/12` 的 scram 规则，覆盖所有库/用户，**新项目无需改 PG 配置**；
- 项目角色只给 `LOGIN`，不要给 SUPERUSER/CREATEDB/CREATEROLE；
- 需要独立实例/端口时，另建 DATADIR（如 `postgresql@.service`），不要改本实例端口；
- 不要改动 `postgres`、`lkb` 角色与 `lkb`、`lkb_test` 库。

## 7. IP 变化 / URL 更新

- 默认 `localhost`：WSL 重启后无需任何更新；
- 若切换/回退到直连 IP 且 IP 已变化：WSL 内 root 跑 `lkb-pg-env-refresh.sh auto`（自动取新 IP 并重写三处 env）；
- 若 localhost 转发失效（Windows 上探测 `127.0.0.1:5432` 失败）：改用 IP 方式（同上），必要时把 `lkb-pg-env-refresh.sh` 固定传入 host；
- 探测命令（PowerShell）：

```powershell
$c = New-Object Net.Sockets.TcpClient
try { $c.Connect('127.0.0.1', 5432); 'OPEN' } catch { 'FAIL: ' + $_.Exception.Message }
```

## 8. 故障排查

| 症状 | 检查 |
| --- | --- |
| 连接被拒（Windows 侧） | `wsl -d linxira-2026-10-27 -e systemctl status postgresql`、`ss -ltn | grep 5432`、`pg_isready` |
| 服务未启动 | `systemctl start postgresql`；`journalctl -u postgresql -n 50` |
| 服务启动失败 | 数据目录是否初始化（`postgresql-check-db-dir`）；`/var/lib/postgres/data` 权限应为 postgres:postgres 0700 |
| `localhost` 不通 | 换 WSL IP 直连；确认 WSL 正在运行（`wsl -l -v`） |
| 认证失败 | `lkb-pg-status.sh` 确认 `lkb` 为 `scram=true`；核对/env 重建：`lkb-pg-provision.sh`（会读 env 中密码） |
| env 文件权限 | WSL 侧 `0600`；Windows 侧 `icacls F:\CODE\RAG\.lkb\pg.env` 应仅当前用户 |
| 迁移失败 | 本地跑集成测试复现；结合 `journalctl -u postgresql -n 100` 定位 SQL |
| 磁盘 | `df -h /`（当前约 912G 可用） |

## 9. 验证记录（2026-11-09）

- 服务：`systemctl is-enabled postgresql` → `enabled`；`is-active` → `active`；`ss -ltn` → `0.0.0.0:5432` 与 `[::]:5432`；`psql --version` → 18.6。
- Windows 连通性探测（TcpClient）：`127.0.0.1:5432` 与 `172.23.85.43:5432` 均为 OPEN（`::1` 无转发监听，不影响使用）。
- 迁移：`lkb_test.schema_migrations` 含 `0001_init … 0011_sso` 全部 11 项（43 张表；`idx_chunks_tsv`、`idx_users_email`、`idx_users_external` 等索引存在；`users` 含 `email/external_id/source/last_sso_at`）。`lkb` 库保持空白（0 张表），等待应用首次启动时自行执行迁移。
- 聚焦测试（真实 PG，未 SKIP）：
  `cargo +stable-x86_64-pc-windows-gnu test --test postgres_integration -- --nocapture`
  → `1 passed; 0 failed`（约 1.15s；输出中无 “SKIP postgres integration test”），链路为 connect → migrate → import examples → BM25 搜索命中。
- 全量测试：`cargo +stable-x86_64-pc-windows-gnu test` → **152 passed; 0 failed**（17 个集成测试文件；`postgres_integration` 复跑同样通过）。
- 期间修复的真实问题：`src/store/postgres.rs::execute_script` 原先按 `sql.split(';')` 客户端切分迁移脚本，被 `migrations/postgres/0002_search.sql` 注释中的分号截断，PostgreSQL 报 `42601 syntax error at or near "the"`。已改为整个脚本一次下发、由服务器解析（simple-query 协议）；注意调用形式必须是 `sqlx::Executor::execute(connection, sqlx::raw_sql(sql))`，`RawSql::execute` 在 async 上下文会触发高阶生命周期推断失败（sqlx#3581）。

