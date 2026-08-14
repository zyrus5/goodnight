import { useEffect, useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import { errorMessage, http } from "../lib/http";
import { getPage, type Resource } from "../services/api";

export function TasksPage() {
  const [items, setItems] = useState<Resource[]>([]);
  const [query, setQuery] = useState("");
  const [error, setError] = useState("");
  const [busyId, setBusyId] = useState("");
  const navigate = useNavigate();
  const load = (term = query) =>
    void getPage<Resource>("/tasks", term)
      .then((result) => {
        setItems(result.items);
        setError("");
      })
      .catch((requestError) => setError(errorMessage(requestError)));
  useEffect(() => {
    const timer = window.setTimeout(() => load(query), 250);
    return () => window.clearTimeout(timer);
  }, [query]);

  async function run(task: Resource) {
    setBusyId(task.id);
    setError("");
    try {
      const response = await http.post(`/tasks/${task.id}/run`, null, {
        headers: { "Idempotency-Key": crypto.randomUUID() },
      });
      navigate(`/executions/${response.data.id}`);
    } catch (requestError) {
      setError(errorMessage(requestError));
      setBusyId("");
    }
  }

  async function remove(task: Resource) {
    if (!confirm(`确定删除任务“${String(task.name)}”吗？历史执行记录会保留。`))
      return;
    setBusyId(task.id);
    setError("");
    try {
      await http.delete(`/tasks/${task.id}`);
      load(query);
    } catch (requestError) {
      setError(errorMessage(requestError));
    } finally {
      setBusyId("");
    }
  }

  async function togglePin(task: Resource) {
    setBusyId(task.id);
    setError("");
    try {
      await http.post(`/tasks/${task.id}/pin`);
      load(query);
    } catch (requestError) {
      setError(errorMessage(requestError));
    } finally {
      setBusyId("");
    }
  }

  return (
    <>
      <div className="page-heading compact">
        <div>
          <p className="eyebrow">DAG ORCHESTRATION</p>
          <h1>任务中心</h1>
          <p>同一层级的 Job 并行执行，下游会等待所有前置节点成功。</p>
        </div>
        <Link className="primary button" to="/tasks/new">
          ＋ 创建任务
        </Link>
      </div>
      {error && <div className="alert error">{error}</div>}
      <section className="panel table-panel">
        <div className="table-tools">
          <label className="search">
            ⌕
            <input
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder="按任务名称搜索…"
            />
          </label>
          <span>共 {items.length} 条</span>
        </div>
        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                <th>任务名称</th>
                <th>触发方式</th>
                <th>下一次执行</th>
                <th>状态</th>
                <th>创建人</th>
                <th>更新时间</th>
                <th>操作</th>
              </tr>
            </thead>
            <tbody>
              {items.length === 0 ? (
                <tr>
                  <td colSpan={7} className="empty">
                    {query
                      ? "没有匹配的任务"
                      : "暂无任务，创建第一条发布编排吧"}
                  </td>
                </tr>
              ) : (
                items.map((task) => (
                  <tr key={task.id}>
                    <td>
                      {Boolean(task.pinned_at) && <span className="pin-marker" title="已置顶">◆</span>}
                      <strong>{String(task.name)}</strong>
                      <small className="cell-sub">
                        {String(task.description ?? "")}
                      </small>
                    </td>
                    <td>{trigger(String(task.trigger_type))}</td>
                    <td>
                      {task.next_run_at
                        ? new Date(String(task.next_run_at)).toLocaleString()
                        : "—"}
                    </td>
                    <td>
                      <span
                        className={`status ${task.is_enabled ? "success" : "muted"}`}
                      >
                        {task.is_enabled ? "启用" : "禁用"}
                      </span>
                    </td>
                    <td>{String(task.creator_name)}</td>
                    <td>
                      {new Date(String(task.updated_at)).toLocaleString()}
                    </td>
                    <td>
                      <div className="row-actions">
                        <button
                          className={`link-button ${task.pinned_at ? "pin-active" : ""}`}
                          disabled={busyId === task.id}
                          onClick={() => void togglePin(task)}
                        >
                          {task.pinned_at ? "取消置顶" : "置顶"}
                        </button>
                        <Link to={`/tasks/${task.id}`}>查看 / 编辑</Link>
                        <button
                          className="link-button"
                          disabled={busyId === task.id}
                          onClick={() => void run(task)}
                        >
                          运行
                        </button>
                        <button
                          className="link-button danger-text"
                          disabled={busyId === task.id}
                          onClick={() => void remove(task)}
                        >
                          删除
                        </button>
                      </div>
                    </td>
                  </tr>
                ))
              )}
            </tbody>
          </table>
        </div>
      </section>
    </>
  );
}

function trigger(value: string) {
  return (
    { IMMEDIATE: "立即执行", ONCE: "指定时间", CRON: "周期执行" }[value] ??
    value
  );
}
