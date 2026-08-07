import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { errorMessage, http } from "../lib/http";
import { getPage, type Resource } from "../services/api";

export function ExecutionsPage() {
  const [items, setItems] = useState<Resource[]>([]);
  const [error, setError] = useState("");

  const load = () =>
    void getPage<Resource>("/executions")
      .then((page) => {
        setItems(page.items);
        setError("");
      })
      .catch((requestError) => setError(errorMessage(requestError)));

  useEffect(load, []);

  async function remove(execution: Resource) {
    if (!window.confirm(`确定物理删除“${String(execution.task_name)}”的这条执行记录吗？`)) return;
    try {
      await http.delete(`/executions/${execution.id}`);
      load();
    } catch (requestError) {
      setError(errorMessage(requestError));
    }
  }

  return <>
    <div className="page-heading compact"><div><p className="eyebrow">EXECUTION HISTORY</p><h1>执行记录</h1><p>不可变的触发快照与 Jenkins 运行结果。</p></div></div>
    {error && <div className="alert error">{error}</div>}
    <section className="panel table-panel"><div className="table-wrap"><table>
      <thead><tr><th>任务</th><th>触发</th><th>状态</th><th>计划时间</th><th>开始时间</th><th>结束时间</th><th>操作</th></tr></thead>
      <tbody>{items.map((execution) => {
        const ended = ["SUCCESS", "FAILED", "CANCELED"].includes(String(execution.status));
        return <tr key={execution.id}>
          <td><strong>{String(execution.task_name)}</strong></td><td>{String(execution.trigger_type)}</td>
          <td><span className={`status ${String(execution.status).toLowerCase()}`}>{String(execution.status)}</span></td>
          <td>{date(execution.scheduled_at)}</td><td>{date(execution.started_at)}</td><td>{date(execution.finished_at)}</td>
          <td><div className="row-actions"><Link to={`/executions/${execution.id}`}>查看节点</Link>{ended && <button className="link-button danger-text" onClick={() => void remove(execution)}>删除</button>}</div></td>
        </tr>;
      })}</tbody>
    </table></div></section>
  </>;
}

const date = (value: unknown) => value ? new Date(String(value)).toLocaleString() : "—";
