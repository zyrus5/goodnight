import { FormEvent, useEffect, useMemo, useState, type DragEvent } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { errorMessage, http } from "../lib/http";
import { getPage, type Page, type Resource } from "../services/api";
import { SearchableSelect } from "../components/SearchableSelect";

interface NodeDef {
  key: string;
  name: string;
  job_config_id: string;
  dependencies: string[];
  parameters: Record<string, unknown>;
  timeout_seconds: number;
}

export function TaskEditorPage() {
  const { id } = useParams();
  const navigate = useNavigate();
  const [jobs, setJobs] = useState<Resource[]>([]);
  const [jobTotal, setJobTotal] = useState(0);
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [trigger, setTrigger] = useState("IMMEDIATE");
  const [scheduled, setScheduled] = useState("");
  const [cron, setCron] = useState("0 2 * * *");
  const [timezone, setTimezone] = useState("Asia/Shanghai");
  const [nodes, setNodes] = useState<NodeDef[]>([]);
  const [version, setVersion] = useState<number>();
  const [preview, setPreview] = useState<string[]>([]);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [jobQuery, setJobQuery] = useState("");
  const [componentFilter, setComponentFilter] = useState("");
  const [customerFilter, setCustomerFilter] = useState("");
  const [deploymentDomainFilter, setDeploymentDomainFilter] = useState("");
  const [environmentCodeFilter, setEnvironmentCodeFilter] = useState("");

  useEffect(() => {
    if (id)
      void http.get(`/tasks/${id}`).then(({ data }) => {
        setName(data.name);
        setDescription(data.description);
        setTrigger(data.trigger_type);
        setScheduled(data.scheduled_at?.slice(0, 16) ?? "");
        setCron(data.cron_expression ?? "0 2 * * *");
        setTimezone(data.timezone);
        setNodes(normalizeLevelDependencies(data.definition.nodes ?? []));
        setVersion(data.version);
      });
  }, [id]);
  useEffect(() => {
    const timer = window.setTimeout(() => {
      void http.post<Page<Resource>>("/job-configs/search", {
        q: jobQuery,
        component_id: componentFilter || null,
        customer_id: customerFilter || null,
        deployment_domain: deploymentDomainFilter,
        environment_code: environmentCodeFilter,
        page: 1,
        page_size: 100,
      })
        .then((response) => {
          setJobs(response.data.items);
          setJobTotal(response.data.total);
          setError("");
        })
        .catch((requestError) => setError(errorMessage(requestError)));
    }, 250);
    return () => window.clearTimeout(timer);
  }, [jobQuery, componentFilter, customerFilter, deploymentDomainFilter, environmentCodeFilter]);
  useEffect(() => {
    if (trigger === "CRON")
      void http
        .post("/tasks/cron-preview", { cron_expression: cron, timezone })
        .then((response) => setPreview(response.data.times))
        .catch(() => setPreview([]));
  }, [trigger, cron, timezone]);

  const visibleJobs = jobs;
  const levels = useMemo(() => topologicalLevels(nodes), [nodes]);

  function nodeFor(job: Resource, dependencies: string[]): NodeDef {
    return {
      key: crypto.randomUUID(),
      name: String(job.display_name),
      job_config_id: job.id,
      dependencies,
      parameters: (job.parameter_presets as Record<string, unknown>) ?? {},
      timeout_seconds: 3600,
    };
  }
  function add(job: Resource, dependencies: string[] = []) {
    setNodes((current) =>
      normalizeLevelDependencies([...current, nodeFor(job, dependencies)]),
    );
  }
  function startDrag(event: DragEvent<HTMLButtonElement>, job: Resource) {
    event.dataTransfer.effectAllowed = "copy";
    event.dataTransfer.setData("application/x-goodnight-job", job.id);
  }
  function drop(event: DragEvent<HTMLElement>, dependencies: string[]) {
    event.preventDefault();
    event.stopPropagation();
    const job = jobs.find(
      (item) =>
        item.id === event.dataTransfer.getData("application/x-goodnight-job"),
    );
    if (job) add(job, dependencies);
  }
  function update(key: string, patch: Partial<NodeDef>) {
    setNodes((current) =>
      normalizeLevelDependencies(
        current.map((node) =>
          node.key === key ? { ...node, ...patch } : node,
        ),
      ),
    );
  }
  function remove(key: string) {
    setNodes((current) =>
      normalizeLevelDependencies(
        current
          .filter((node) => node.key !== key)
          .map((node) => ({
            ...node,
            dependencies: node.dependencies.filter(
              (dependency) => dependency !== key,
            ),
          })),
      ),
    );
  }

  async function submit(event: FormEvent) {
    event.preventDefault();
    if (nodes.length === 0) {
      setError("请至少添加一个 Job 节点");
      return;
    }
    setBusy(true);
    setError("");
    const payload = {
      name,
      description,
      trigger_type: trigger,
      scheduled_at:
        trigger === "ONCE" ? new Date(scheduled).toISOString() : null,
      cron_expression: trigger === "CRON" ? cron : null,
      timezone,
      is_enabled: true,
      definition: { nodes },
      version,
    };
    try {
      if (id) {
        await http.put(`/tasks/${id}`, payload);
        navigate("/tasks");
      } else {
        const response = await http.post("/tasks", payload, {
          headers: { "Idempotency-Key": crypto.randomUUID() },
        });
        if (response.data.execution_id)
          navigate(`/executions/${response.data.execution_id}`);
        else navigate("/tasks");
      }
    } catch (requestError) {
      setError(errorMessage(requestError));
    } finally {
      setBusy(false);
    }
  }

  return (
    <form onSubmit={submit}>
      <div className="page-heading compact">
        <div>
          <p className="eyebrow">TASK DESIGNER</p>
          <h1>{id ? "编辑任务" : "创建任务"}</h1>
          <p>同层节点并行执行；下一层会等待上一层全部成功后执行。</p>
        </div>
        <div className="actions">
          <button type="button" onClick={() => navigate("/tasks")}>
            取消
          </button>
          <button className="primary" disabled={busy}>
            {busy
              ? "保存中…"
              : trigger === "IMMEDIATE" && !id
                ? "保存并执行"
                : "保存任务"}
          </button>
        </div>
      </div>
      {error && <div className="alert error">{error}</div>}
      <div className="task-editor-layout">
      <section className="panel task-meta">
        <label>
          任务名称
          <input
            required
            value={name}
            onChange={(event) => setName(event.target.value)}
            placeholder="例如：多环境并行发布"
          />
        </label>
        <label>
          触发方式
          <select
            value={trigger}
            onChange={(event) => setTrigger(event.target.value)}
          >
            <option value="IMMEDIATE">立即执行</option>
            <option value="ONCE">指定时间</option>
            <option value="CRON">周期执行</option>
          </select>
        </label>
        <label className="full">
          说明
          <textarea
            value={description}
            onChange={(event) => setDescription(event.target.value)}
          />
        </label>
        {trigger === "ONCE" && (
          <label>
            计划时间
            <input
              type="datetime-local"
              required
              value={scheduled}
              onChange={(event) => setScheduled(event.target.value)}
            />
          </label>
        )}
        {trigger === "CRON" && (
          <>
            <label>
              Cron（五段）
              <input
                required
                value={cron}
                onChange={(event) => setCron(event.target.value)}
              />
            </label>
            <label>
              时区
              <select
                value={timezone}
                onChange={(event) => setTimezone(event.target.value)}
              >
                <option>Asia/Shanghai</option>
                <option>UTC</option>
                <option>Asia/Tokyo</option>
                <option>America/New_York</option>
              </select>
            </label>
            <div className="cron-preview">
              <strong>未来 5 次</strong>
              {preview.map((value) => (
                <span key={value}>{new Date(value).toLocaleString()}</span>
              ))}
            </div>
          </>
        )}
      </section>
      <div className="designer">
        <aside className="job-palette panel">
          <div className="panel-title">
            <strong>可用 Job</strong>
            <span>
              {visibleJobs.length} / {jobTotal}
            </span>
          </div>
          <div className="job-palette-filters">
            <input
              value={jobQuery}
              onChange={(event) => setJobQuery(event.target.value)}
              placeholder="搜索 Job"
            />
            <SearchableSelect
              ariaLabel="组件"
              value={componentFilter}
              onChange={setComponentFilter}
              placeholder="搜索组件"
              emptyLabel="全部组件"
              options={[]}
              loadOptions={(query) => resourceOptions("/components", query, (row) => `${row.code} · ${row.name}`)}
            />
            <SearchableSelect
              ariaLabel="客户"
              value={customerFilter}
              onChange={setCustomerFilter}
              placeholder="搜索客户"
              emptyLabel="全部客户"
              options={[]}
              loadOptions={(query) => resourceOptions("/customers", query, (row) => `${row.code} · ${row.name}`)}
            />
            <SearchableSelect
              ariaLabel="部署域"
              value={deploymentDomainFilter}
              onChange={setDeploymentDomainFilter}
              placeholder="搜索部署域"
              emptyLabel="全部部署域"
              options={[]}
              loadOptions={deploymentDomainOptions}
            />
            <SearchableSelect
              ariaLabel="环境代码"
              value={environmentCodeFilter}
              onChange={setEnvironmentCodeFilter}
              placeholder="选择环境代码"
              emptyLabel="全部环境代码"
              options={environmentCodeOptions}
            />
          </div>
          <div className="job-palette-list">
            {visibleJobs.map((job) => (
              <button
                type="button"
                key={job.id}
                draggable
                onDragStart={(event) => startDrag(event, job)}
                onClick={() => add(job)}
              >
                <span className="job-icon">▶</span>
                <div>
                  <strong>{String(job.display_name)}</strong>
                  <small>
                    客户：{String(job.customer_name ?? "—")} 环境：
                    {job.deployment_domain
                      ? `${String(job.deployment_domain)}-`
                      : ""}
                    {String(job.environment_name ?? "—")}
                  </small>
                  <small>
                    {String(job.component_name)} · {String(job.instance_name)} ·{" "}
                    {String(job.job_full_name)}
                  </small>
                </div>
                <b>＋</b>
              </button>
            ))}
          </div>
        </aside>
        <section className="canvas panel">
          <div className="panel-title">
            <strong>任务流程</strong>
            <span>
              {nodes.length} 个节点 · {levels.length} 层
            </span>
          </div>
          {nodes.length === 0 ? (
            <div
              className="empty-canvas dag-drop-zone empty-drop-zone"
              onDragOver={(event) => event.preventDefault()}
              onDrop={(event) => drop(event, [])}
            >
              <span>⌘</span>
              <strong>拖入第一个 Job</strong>
              <p>也可以点击左侧 Job；第一层中的节点会并行执行。</p>
            </div>
          ) : (
            <div className="dag-editor">
              {levels.map((level, index) => (
                <div
                  className="dag-level"
                  key={index}
                  onDragOver={(event) => event.preventDefault()}
                  onDrop={(event) =>
                    drop(
                      event,
                      index === 0
                        ? []
                        : levels[index - 1].map((node) => node.key),
                    )
                  }
                >
                  <div className="level-label">
                    层级 {index + 1} ·{" "}
                    {level.length > 1 ? "并行执行" : "单节点"}
                  </div>
                  <div
                    className="dag-drop-zone parallel-drop-zone"
                    onDragOver={(event) => event.preventDefault()}
                    onDrop={(event) =>
                      drop(
                        event,
                        index === 0
                          ? []
                          : levels[index - 1].map((node) => node.key),
                      )
                    }
                  >
                    ＋ 拖到这里，与本层并行
                  </div>
                  {level.map((node) => {
                    const job = jobs.find(
                      (item) => item.id === node.job_config_id,
                    );
                    return <div className="node-card" key={node.key}>
                      <div className="node-head">
                        <span>▶</span>
                        <input
                          value={node.name}
                          onChange={(event) =>
                            update(node.key, { name: event.target.value })
                          }
                        />
                        <button
                          type="button"
                          onClick={() => remove(node.key)}
                        >
                          ×
                        </button>
                      </div>
                      {job && (
                        <div className="node-context">
                          <span>
                            客户：{String(job.customer_name ?? "—")} 环境：
                            {job.deployment_domain
                              ? `${String(job.deployment_domain)}-`
                              : ""}
                            {String(job.environment_name ?? "—")}
                          </span>
                          <small>
                            {String(job.component_name)} ·{" "}
                            {String(job.instance_name)} ·{" "}
                            {String(job.job_full_name)}
                          </small>
                        </div>
                      )}
                      <div className="node-fields">
                        <label>
                          前置层节点（自动等待整层）
                          <select
                            multiple
                            value={node.dependencies}
                            onChange={(event) =>
                              update(node.key, {
                                dependencies: [
                                  ...event.target.selectedOptions,
                                ].map((option) => option.value),
                              })
                            }
                          >
                            {nodes
                              .filter((item) => item.key !== node.key)
                              .map((item) => (
                                <option key={item.key} value={item.key}>
                                  {item.name}
                                </option>
                              ))}
                          </select>
                        </label>
                        <label>
                          超时（秒）
                          <input
                            type="number"
                            min={30}
                            value={node.timeout_seconds}
                            onChange={(event) =>
                              update(node.key, {
                                timeout_seconds: Number(event.target.value),
                              })
                            }
                          />
                        </label>
                        <label className="full">
                          参数 JSON
                          <textarea
                            value={JSON.stringify(node.parameters, null, 2)}
                            onChange={(event) => {
                              try {
                                update(node.key, {
                                  parameters: JSON.parse(event.target.value),
                                });
                              } catch {
                                /* 保留上一次合法 JSON */
                              }
                            }}
                          />
                        </label>
                      </div>
                    </div>;
                  })}
                </div>
              ))}
              <div
                className="dag-drop-zone serial-drop-zone"
                onDragOver={(event) => event.preventDefault()}
                onDrop={(event) =>
                  drop(event, levels.at(-1)?.map((node) => node.key) ?? [])
                }
              >
                ↓ 拖到这里，串行到下一层（等待上一层全部成功）
              </div>
            </div>
          )}
        </section>
      </div>
      </div>
    </form>
  );
}

async function resourceOptions(
  endpoint: string,
  query: string,
  label: (row: Resource) => string,
) {
  const page = await getPage<Resource>(endpoint, query, 1, { page_size: 20 });
  return page.items.map((row) => ({ value: row.id, label: label(row) }));
}
async function deploymentDomainOptions(query: string) {
  const page = await getPage<Resource>("/environments", query, 1, { page_size: 100 });
  return [...new Set(page.items.map((row) => String(row.deployment_domain)).filter(Boolean))]
    .map((value) => ({ value, label: value }));
}
const environmentCodeOptions = [
  { value: "dev", label: "dev · 开发" },
  { value: "test", label: "test · 测试" },
  { value: "uat", label: "uat · 验收" },
  { value: "branch", label: "branch · 分支" },
  { value: "prod", label: "prod · 生产" },
];
function topologicalLevels(nodes: NodeDef[]) {
  const remaining = new Map(nodes.map((node) => [node.key, node]));
  const done = new Set<string>();
  const levels: NodeDef[][] = [];
  while (remaining.size) {
    const level = [...remaining.values()].filter((node) =>
      node.dependencies.every(
        (dependency) => done.has(dependency) || !remaining.has(dependency),
      ),
    );
    if (!level.length) {
      levels.push([...remaining.values()]);
      break;
    }
    levels.push(level);
    for (const node of level) {
      remaining.delete(node.key);
      done.add(node.key);
    }
  }
  return levels;
}

function normalizeLevelDependencies(nodes: NodeDef[]) {
  const levels = topologicalLevelsStrict(nodes);
  if (!levels) return nodes;
  const dependencies = new Map<string, string[]>();
  levels.forEach((level, index) => {
    const previous = index === 0 ? [] : levels[index - 1].map((node) => node.key);
    level.forEach((node) => dependencies.set(node.key, previous));
  });
  return nodes.map((node) => ({
    ...node,
    dependencies: dependencies.get(node.key) ?? node.dependencies,
  }));
}

function topologicalLevelsStrict(nodes: NodeDef[]) {
  const remaining = new Map(nodes.map((node) => [node.key, node]));
  const done = new Set<string>();
  const levels: NodeDef[][] = [];
  while (remaining.size) {
    const level = [...remaining.values()].filter((node) =>
      node.dependencies.every(
        (dependency) => done.has(dependency) || !remaining.has(dependency),
      ),
    );
    if (!level.length) return null;
    levels.push(level);
    for (const node of level) {
      remaining.delete(node.key);
      done.add(node.key);
    }
  }
  return levels;
}
