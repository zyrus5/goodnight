import { useEffect, useMemo, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { errorMessage, http } from "../lib/http";

interface Node {
  id: string;
  node_key: string;
  node_name: string;
  component_name: string;
  customer_name: string;
  deployment_domain: string;
  environment_name: string;
  argo_url?: string;
  wiki_url?: string;
  apollo_url?: string;
  log_url?: string;
  dependencies: string[];
  status: string;
  queue_url?: string;
  build_url?: string;
  blocking_reason?: string;
  error_summary?: string;
}
interface ExecutionData {
  execution: Record<string, unknown>;
  nodes: Node[];
}

export function ExecutionDetailPage() {
  const { id } = useParams();
  const navigate = useNavigate();
  const [data, setData] = useState<ExecutionData>();
  const [log, setLog] = useState<Record<string, string>>({});
  const [instanceInfo, setInstanceInfo] = useState<Record<string, boolean>>({});
  const [error, setError] = useState("");
  const load = () =>
    void http
      .get(`/executions/${id}`)
      .then((response) => {
        setData(response.data);
        setError("");
      })
      .catch((requestError) => setError(errorMessage(requestError)));
  useEffect(() => {
    load();
    const timer = window.setInterval(() => load(), 2000);
    return () => window.clearInterval(timer);
  }, [id]);
  const levels = useMemo(() => executionLevels(data?.nodes ?? []), [data?.nodes]);

  async function logs(node: Node) {
    try {
      const response = await http.get(
        `/executions/${id}/nodes/${node.id}/log`,
      );
      setLog((current) => ({
        ...current,
        [node.id]: (current[node.id] ?? "") + response.data.text,
      }));
    } catch (requestError) {
      setError(errorMessage(requestError));
    }
  }
  async function stop() {
    if (
      confirm(
        "将阻止新节点并尽力停止 Jenkins 中已排队或运行的构建，确认继续？",
      )
    ) {
      await http.post(`/executions/${id}/stop`);
      load();
    }
  }
  async function copy() {
    const response = await http.post(`/executions/${id}/copy`);
    navigate(`/tasks/${response.data.task_id}`);
  }
  if (!data) return <div className="empty">加载执行详情…</div>;
  const status = String(data.execution.status);
  const ended = ["SUCCESS", "FAILED", "CANCELED"].includes(status);
  const activeNodes = data.nodes.filter((node) =>
    ["PENDING", "QUEUED", "RUNNING", "UNKNOWN"].includes(node.status),
  );

  return (
    <>
      <div className="page-heading compact">
        <div>
          <p className="eyebrow">LIVE EXECUTION FLOW</p>
          <h1>{String(data.execution.task_name)}</h1>
          <p>
            {ended
              ? "执行已结束"
              : activeNodes.length
                ? `当前活动：${activeNodes.map((node) => node.node_name).join("、")}`
                : "正在准备执行节点…"}
          </p>
        </div>
        <div className="actions">
          {ended && <button onClick={() => void copy()}>复制为新任务</button>}
          {["RUNNING", "CANCELING", "SCHEDULED"].includes(status) && (
            <button className="danger" onClick={() => void stop()}>
              停止执行
            </button>
          )}
        </div>
      </div>
      {error && <div className="alert error">{error}</div>}
      <div className="execution-summary panel">
        <span className={`status ${status.toLowerCase()}`}>{status}</span>
        <div>
          <small>开始时间</small>
          <strong>
            {data.execution.started_at
              ? new Date(String(data.execution.started_at)).toLocaleString()
              : "—"}
          </strong>
        </div>
        <div>
          <small>结束时间</small>
          <strong>
            {data.execution.finished_at
              ? new Date(String(data.execution.finished_at)).toLocaleString()
              : "—"}
          </strong>
        </div>
        <div>
          <small>刷新频率</small>
          <strong>每 2 秒</strong>
        </div>
      </div>
      <section className="execution-flow panel">
        {levels.map((level, index) => (
          <div className="execution-level" key={index}>
            <div className="execution-level-label">
              <span>层级 {index + 1}</span>
              <small>{level.length > 1 ? "并行节点" : "单节点"}</small>
            </div>
            <div className="execution-level-nodes">
              {level.map((node) => (
                <article
                  className={`execution-flow-node ${node.status.toLowerCase()} ${["QUEUED", "RUNNING"].includes(node.status) ? "current" : ""}`}
                  key={node.id}
                >
                  <header>
                    <span className="job-icon">▶</span>
                    <strong>{node.node_name}</strong>
                    <span className={`status ${node.status.toLowerCase()}`}>
                      {node.status}
                    </span>
                  </header>
                  <div className="node-context">
                    <span>
                      组件：{node.component_name} · 客户：{node.customer_name}
                    </span>
                    <small>
                      部署域：{node.deployment_domain || "—"} · 环境：
                      {node.environment_name}
                    </small>
                  </div>
                  {(node.dependencies ?? []).length > 0 && (
                    <small className="flow-dependencies">
                      等待：
                      {node.dependencies
                        .map(
                          (key) =>
                            data.nodes.find((item) => item.node_key === key)
                              ?.node_name ?? key,
                        )
                        .join("、")}
                    </small>
                  )}
                  {(node.error_summary || node.blocking_reason) && (
                    <div className="node-message">
                      {node.error_summary || node.blocking_reason}
                    </div>
                  )}
                  <div className="node-actions">
                    {node.queue_url && (
                      <a target="_blank" rel="noreferrer" href={node.queue_url}>
                        等待队列 ↗
                      </a>
                    )}
                    {node.build_url && (
                      <a target="_blank" rel="noreferrer" href={node.build_url}>
                        Jenkins 构建 ↗
                      </a>
                    )}
                    <button
                      onClick={() =>
                        setInstanceInfo((current) => ({
                          ...current,
                          [node.id]: !current[node.id],
                        }))
                      }
                    >
                      查看组件实例信息
                    </button>
                    <button onClick={() => void logs(node)}>读取增量日志</button>
                  </div>
                  {instanceInfo[node.id] && (
                    <div className="instance-links">
                      {[
                        ["Wiki 地址", "打开 Wiki", node.wiki_url],
                        ["Argo 地址", "打开 Argo", node.argo_url],
                        ["Apollo 地址", "打开 Apollo", node.apollo_url],
                        ["日志地址", "打开日志", node.log_url],
                      ].map(([label, action, url]) => (
                        <div key={label}>
                          <span>{label}</span>
                          {url ? (
                            <a
                              href={url}
                              target="_blank"
                              rel="noreferrer"
                              title={url}
                            >
                              {action} ↗
                            </a>
                          ) : (
                            <small>未配置</small>
                          )}
                        </div>
                      ))}
                    </div>
                  )}
                  {log[node.id] !== undefined && (
                    <pre className="console">{log[node.id] || "暂无新日志"}</pre>
                  )}
                </article>
              ))}
            </div>
            {index < levels.length - 1 && (
              <div className="flow-arrow">↓ 前置节点全部成功后继续</div>
            )}
          </div>
        ))}
      </section>
    </>
  );
}

function executionLevels(nodes: Node[]) {
  const remaining = new Map(nodes.map((node) => [node.node_key, node]));
  const done = new Set<string>();
  const levels: Node[][] = [];
  while (remaining.size) {
    const level = [...remaining.values()].filter((node) =>
      (node.dependencies ?? []).every(
        (dependency) => done.has(dependency) || !remaining.has(dependency),
      ),
    );
    if (!level.length) {
      levels.push([...remaining.values()]);
      break;
    }
    levels.push(level);
    for (const node of level) {
      remaining.delete(node.node_key);
      done.add(node.node_key);
    }
  }
  return levels;
}
