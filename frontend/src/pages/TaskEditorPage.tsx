import { FormEvent, useEffect, useMemo, useState, type DragEvent } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { errorMessage, http } from "../lib/http";
import { getPage, type Resource } from "../services/api";
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
  const [environmentFilter, setEnvironmentFilter] = useState("");

  useEffect(() => {
    void Promise.all([
      getPage<Resource>("/job-configs", "", 1, { page_size: 100 }),
      getPage<Resource>("/component-instances", "", 1, { page_size: 100 }),
    ]).then(([jobPage, instancePage]) => {
      const instances = new Map(
        instancePage.items.map((instance) => [instance.id, instance]),
      );
      setJobs(
        jobPage.items.map((job) => {
          const instance = instances.get(String(job.component_instance_id));
          return {
            ...job,
            component_name: job.component_name ?? instance?.component_name,
            customer_id: job.customer_id ?? instance?.customer_id,
            customer_name: job.customer_name ?? instance?.customer_name,
            deployment_domain:
              job.deployment_domain ?? instance?.deployment_domain,
            environment_id: job.environment_id ?? instance?.environment_id,
            environment_name:
              job.environment_name ?? instance?.environment_name,
          };
        }),
      );
    });
    if (id)
      void http.get(`/tasks/${id}`).then(({ data }) => {
        setName(data.name);
        setDescription(data.description);
        setTrigger(data.trigger_type);
        setScheduled(data.scheduled_at?.slice(0, 16) ?? "");
        setCron(data.cron_expression ?? "0 2 * * *");
        setTimezone(data.timezone);
        setNodes(data.definition.nodes ?? []);
        setVersion(data.version);
      });
  }, [id]);
  useEffect(() => {
    if (trigger === "CRON")
      void http
        .post("/tasks/cron-preview", { cron_expression: cron, timezone })
        .then((response) => setPreview(response.data.times))
        .catch(() => setPreview([]));
  }, [trigger, cron, timezone]);

  const components = uniqueOptions(jobs, "component_id", "component_name");
  const customers = uniqueOptions(jobs, "customer_id", "customer_name");
  const deploymentDomains = uniqueOptions(
    jobs,
    "deployment_domain",
    "deployment_domain",
  );
  const environments = uniqueOptions(
    jobs,
    "environment_id",
    "environment_name",
  );
  const visibleJobs = jobs.filter(
    (job) =>
      (!componentFilter || job.component_id === componentFilter) &&
      (!customerFilter || job.customer_id === customerFilter) &&
      (!deploymentDomainFilter ||
        job.deployment_domain === deploymentDomainFilter) &&
      (!environmentFilter || job.environment_id === environmentFilter) &&
      `${job.display_name} ${job.instance_name} ${job.job_full_name}`
        .toLowerCase()
        .includes(jobQuery.trim().toLowerCase()),
  );
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
    setNodes((current) => [...current, nodeFor(job, dependencies)]);
  }
  function startDrag(event: DragEvent<HTMLButtonElement>, job: Resource) {
    event.dataTransfer.effectAllowed = "copy";
    event.dataTransfer.setData("application/x-goodnight-job", job.id);
  }
  function drop(event: DragEvent<HTMLElement>, dependencies: string[]) {
    event.preventDefault();
    const job = jobs.find(
      (item) =>
        item.id === event.dataTransfer.getData("application/x-goodnight-job"),
    );
    if (job) add(job, dependencies);
  }
  function update(key: string, patch: Partial<NodeDef>) {
    setNodes((current) =>
      current.map((node) => (node.key === key ? { ...node, ...patch } : node)),
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
          <p>同层节点并行执行；设置前置节点即可组成串行或混合流程。</p>
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
              {visibleJobs.length} / {jobs.length}
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
              options={components}
            />
            <SearchableSelect
              ariaLabel="客户"
              value={customerFilter}
              onChange={setCustomerFilter}
              placeholder="搜索客户"
              emptyLabel="全部客户"
              options={customers}
            />
            <SearchableSelect
              ariaLabel="部署域"
              value={deploymentDomainFilter}
              onChange={setDeploymentDomainFilter}
              placeholder="搜索部署域"
              emptyLabel="全部部署域"
              options={deploymentDomains}
            />
            <SearchableSelect
              ariaLabel="环境"
              value={environmentFilter}
              onChange={setEnvironmentFilter}
              placeholder="搜索环境"
              emptyLabel="全部环境"
              options={environments}
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
                    {String(job.component_name)} ·{" "}
                    {String(job.environment_name)}
                  </small>
                  <small>
                    {String(job.instance_name)} · {String(job.job_full_name)}
                  </small>
                </div>
                <b>＋</b>
              </button>
            ))}
          </div>
        </aside>
        <section className="canvas panel">
          <div className="panel-title">
            <strong>并行编排流程</strong>
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
                <div className="dag-level" key={index}>
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
                  {level.map((node) => (
                    <div className="node-card" key={node.key}>
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
                          onClick={() =>
                            setNodes((current) =>
                              current.filter((item) => item.key !== node.key),
                            )
                          }
                        >
                          ×
                        </button>
                      </div>
                      <div className="node-fields">
                        <label>
                          前置节点（可多选）
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
                    </div>
                  ))}
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
    </form>
  );
}

function uniqueOptions(items: Resource[], valueKey: string, labelKey: string) {
  return [
    ...new Map(
      items
        .filter((item) => item[valueKey])
        .map((item) => [
          String(item[valueKey]),
          String(item[labelKey] ?? item[valueKey]),
        ]),
    ).entries(),
  ].map(([value, label]) => ({ value, label }));
}
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
