import { FormEvent, useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { errorMessage, http } from "../lib/http";
import { SearchableSelect } from "./SearchableSelect";
import { useAppStore } from "../stores/app";
import {
  createResource,
  deleteResource,
  getPage,
  type Resource,
  updateResource,
} from "../services/api";

export interface Column {
  key: string;
  label: string;
  render?: (value: unknown, row: Resource) => React.ReactNode;
}
export interface Choice {
  value: string;
  label: string;
  values?: Record<string, unknown>;
}
export interface Field {
  key: string;
  label: string;
  type?:
    | "text"
    | "password"
    | "textarea"
    | "checkbox"
    | "number"
    | "select"
    | "multiselect"
    | "json";
  required?: boolean;
  immutable?: boolean;
  placeholder?: string;
  defaultValue?: unknown;
  full?: boolean;
  choices?: Choice[];
  options?: { endpoint: string; label: (row: Resource) => string };
}
export interface Filter {
  key: string;
  label: string;
  type?: "text" | "select";
  placeholder?: string;
  options?: { endpoint: string; label: (row: Resource) => string };
}
export interface ResourceConfig {
  title: string;
  description: string;
  endpoint: string;
  columns: Column[];
  fields?: Field[];
  filters?: Filter[];
  createLabel?: string;
  adminOnly?: boolean;
  copyable?: boolean;
  associateComponents?: boolean;
  deletable?: boolean;
  manageJobs?: boolean;
  createDisabled?: boolean;
  editDisabled?: boolean;
  maintainerOnly?: boolean;
  memberRoles?: { role: "DEVELOPER" | "TESTER"; label: string; field: string }[];
}

export function ResourcePage({ config }: { config: ResourceConfig }) {
  const user = useAppStore((state) => state.user);
  const canMaintain = Boolean(
    user?.is_admin || ["ADMIN", "OPS", "DEVELOPER"].includes(user?.role ?? ""),
  );
  const mutationVisible =
    (!config.adminOnly || Boolean(user?.is_admin)) &&
    (!config.maintainerOnly || canMaintain);
  const [data, setData] = useState<Resource[]>([]);
  const [total, setTotal] = useState(0);
  const [q, setQ] = useState("");
  const [filters, setFilters] = useState<Record<string, string>>({});
  const [filterOptions, setFilterOptions] = useState<
    Record<string, Resource[]>
  >({});
  const [page, setPage] = useState(1);
  const [loading, setLoading] = useState(true);
  const [editing, setEditing] = useState<Resource | null | undefined>(
    undefined,
  );
  const [copying, setCopying] = useState(false);
  const [associating, setAssociating] = useState<Resource | undefined>();
  const [managingJobs, setManagingJobs] = useState<Resource | undefined>();
  const [memberEditing, setMemberEditing] = useState<
    { row: Resource; role: "DEVELOPER" | "TESTER"; label: string; field: string } | undefined
  >();
  const [error, setError] = useState("");
  const reload = () => {
    setLoading(true);
    void getPage<Resource>(config.endpoint, q, page, filters)
      .then((r) => {
        setData(r.items);
        setTotal(r.total);
        setError("");
      })
      .catch((e) => setError(errorMessage(e)))
      .finally(() => setLoading(false));
  };
  useEffect(reload, [config.endpoint, q, page, filters]);
  useEffect(() => {
    for (const f of config.filters ?? []) {
      if (f.options)
        void getPage<Resource>(f.options.endpoint, "", 1, {
          page_size: 100,
        }).then((r) => setFilterOptions((o) => ({ ...o, [f.key]: r.items })));
    }
  }, [config.filters]);
  const setFilter = (key: string, value: string) => {
    setFilters((current) => {
      const next = { ...current };
      if (value) next[key] = value;
      else delete next[key];
      return next;
    });
    setPage(1);
  };
  const openCreate = () => {
    setCopying(false);
    setEditing(null);
  };
  async function remove(row: Resource) {
    if (
      !window.confirm(
        `确定删除“${String(row.name ?? row.code)}”吗？此操作不可撤销。`,
      )
    )
      return;
    setError("");
    try {
      await deleteResource(config.endpoint, row.id);
      reload();
    } catch (e) {
      setError(errorMessage(e));
    }
  }
  return (
    <>
      <div className="page-heading compact">
        <div>
          <p className="eyebrow">PLATFORM RESOURCE</p>
          <h1>{config.title}</h1>
          <p>{config.description}</p>
        </div>
        {config.fields && !config.createDisabled && mutationVisible && (
          <button className="primary" onClick={openCreate}>
            ＋ {config.createLabel ?? "新建"}
          </button>
        )}
      </div>
      <section className="panel table-panel">
        <div className="table-tools">
          <div className="table-filters">
            <div className="search">
              ⌕
              <input
                value={q}
                onChange={(e) => {
                  setQ(e.target.value);
                  setPage(1);
                }}
                placeholder="输入代码或名称搜索…"
              />
            </div>
            {config.filters?.map((f) =>
              f.type === "select" && isSearchableRelation(f.key) ? (
                <SearchableSelect
                  key={f.key}
                  ariaLabel={f.label}
                  value={filters[f.key] ?? ""}
                  onChange={(value) => setFilter(f.key, value)}
                  placeholder={`搜索${f.label}`}
                  emptyLabel={`全部${f.label}`}
                  options={(filterOptions[f.key] ?? []).map((option) => ({
                    value: option.id,
                    label: f.options?.label(option) ?? option.id,
                  }))}
                />
              ) : f.type === "select" ? (
                <select
                  key={f.key}
                  aria-label={f.label}
                  value={filters[f.key] ?? ""}
                  onChange={(e) => setFilter(f.key, e.target.value)}
                >
                  <option value="">全部{f.label}</option>
                  {(filterOptions[f.key] ?? []).map((o) => (
                    <option key={o.id} value={o.id}>
                      {f.options?.label(o)}
                    </option>
                  ))}
                </select>
              ) : (
                <input
                  key={f.key}
                  aria-label={f.label}
                  value={filters[f.key] ?? ""}
                  onChange={(e) => setFilter(f.key, e.target.value)}
                  placeholder={f.placeholder ?? f.label}
                />
              ),
            )}
          </div>
          <span>共 {total} 条</span>
        </div>
        {error && <div className="alert error">{error}</div>}
        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                {config.columns.map((c) => (
                  <th key={c.key}>{c.label}</th>
                ))}
                {(config.fields || config.manageJobs || config.deletable || config.memberRoles) && <th>操作</th>}
              </tr>
            </thead>
            <tbody>
              {loading ? (
                <tr>
                  <td colSpan={99} className="empty">
                    正在加载…
                  </td>
                </tr>
              ) : data.length === 0 ? (
                <tr>
                  <td colSpan={99} className="empty">
                    暂无数据
                  </td>
                </tr>
              ) : (
                data.map((row) => (
                  <tr key={row.id}>
                    {config.columns.map((c) => (
                      <td key={c.key}>
                        {c.render
                          ? c.render(row[c.key], row)
                          : render(row[c.key])}
                      </td>
                    ))}
                    {(config.fields || config.manageJobs || config.deletable || config.memberRoles) && (
                      <td>
                        <div className="row-actions">
                          {!config.editDisabled && mutationVisible && <button
                            className="link-button"
                            onClick={() => {
                              setCopying(false);
                              setEditing(row);
                            }}
                          >
                            编辑
                          </button>}
                          {config.copyable && mutationVisible && (
                            <button
                              className="link-button"
                              onClick={() => {
                                setCopying(true);
                                setEditing(row);
                              }}
                            >
                              复制
                            </button>
                          )}
                          {config.associateComponents && mutationVisible && (
                            <button
                              className="link-button"
                              onClick={() => setAssociating(row)}
                            >
                              关联组件
                            </button>
                          )}
                          {config.manageJobs && (
                            <button
                              className="link-button"
                              onClick={() => setManagingJobs(row)}
                            >
                              管理Job
                            </button>
                          )}
                          {!row.is_public && config.memberRoles?.map((memberRole) => (
                            <button
                              key={memberRole.role}
                              className="link-button"
                              onClick={() => setMemberEditing({ row, ...memberRole })}
                            >
                              {memberRole.label}
                            </button>
                          ))}
                          {config.deletable && mutationVisible && (
                            <button
                              className="link-button danger-text"
                              onClick={() => void remove(row)}
                            >
                              删除
                            </button>
                          )}
                        </div>
                      </td>
                    )}
                  </tr>
                ))
              )}
            </tbody>
          </table>
        </div>
        <div className="pagination">
          <button disabled={page <= 1} onClick={() => setPage((p) => p - 1)}>
            上一页
          </button>
          <span>第 {page} 页</span>
          <button
            disabled={page * 20 >= total}
            onClick={() => setPage((p) => p + 1)}
          >
            下一页
          </button>
        </div>
      </section>
      {editing !== undefined && config.fields && (
        <ResourceDialog
          config={config}
          value={editing}
          createFromCopy={copying}
          onClose={() => setEditing(undefined)}
          onSaved={() => {
            setEditing(undefined);
            reload();
          }}
        />
      )}
      {associating && (
        <EnvironmentAssociationDialog
          environment={associating}
          onClose={() => setAssociating(undefined)}
          onSaved={() => {
            setAssociating(undefined);
            reload();
          }}
        />
      )}
      {managingJobs && (
        <ManageJobsDialog
          instance={managingJobs}
          onClose={() => setManagingJobs(undefined)}
        />
      )}
      {memberEditing && (
        <ComponentMembersDialog
          value={memberEditing}
          onClose={() => setMemberEditing(undefined)}
          onSaved={() => { setMemberEditing(undefined); reload(); }}
        />
      )}
    </>
  );
}

function ComponentMembersDialog({
  value,
  onClose,
  onSaved,
}: {
  value: { row: Resource; role: "DEVELOPER" | "TESTER"; label: string; field: string };
  onClose: () => void;
  onSaved: () => void;
}) {
  const [users, setUsers] = useState<Resource[]>([]);
  const [selected, setSelected] = useState<string[]>(
    Array.isArray(value.row[value.field]) ? (value.row[value.field] as string[]) : [],
  );
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [leftQuery, setLeftQuery] = useState("");
  const [rightQuery, setRightQuery] = useState("");
  useEffect(() => {
    void getPage<Resource>("/users", "", 1, { page_size: 100 })
      .then((page) => setUsers(page.items.filter((item) => item.is_active !== false)))
      .catch((err) => setError(errorMessage(err)));
  }, []);
  async function save() {
    setBusy(true);
    setError("");
    try {
      await http.put(`/components/${value.row.id}/members/${value.role}`, { user_ids: selected });
      onSaved();
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  }
  const matches = (item: Resource, query: string) =>
    `${String(item.display_name)} ${String(item.username)} ${String(item.role)}`
      .toLowerCase().includes(query.trim().toLowerCase());
  const available = users.filter((item) => !selected.includes(item.id) && matches(item, leftQuery));
  const added = users.filter((item) => selected.includes(item.id) && matches(item, rightQuery));
  return <div className="modal-backdrop" onMouseDown={(event) => { if (event.target === event.currentTarget) onClose(); }}>
    <div className="modal transfer-modal">
      <div className="modal-head"><div><p className="eyebrow">COMPONENT MEMBERS</p><h2>{value.label} · {String(value.row.name)}</h2></div><button type="button" onClick={onClose}>×</button></div>
      <div className="transfer-box">
        <section><header><strong>用户列表</strong><span>{available.length}</span></header><div className="transfer-search">⌕<input value={leftQuery} onChange={(event) => setLeftQuery(event.target.value)} placeholder="搜索用户" /></div><div className="transfer-list">{available.map((item) => <button type="button" key={item.id} onClick={() => setSelected((current) => [...current, item.id])}><span><strong>{String(item.display_name)}</strong><small>{String(item.username)} · {String(item.role)}</small></span><b>＋</b></button>)}{available.length === 0 && <div className="association-empty">暂无可添加用户</div>}</div></section>
        <div className="transfer-arrows"><span>→</span><span>←</span></div>
        <section><header><strong>已添加用户</strong><span>{added.length}</span></header><div className="transfer-search">⌕<input value={rightQuery} onChange={(event) => setRightQuery(event.target.value)} placeholder="搜索已添加用户" /></div><div className="transfer-list">{added.map((item) => <button type="button" key={item.id} onClick={() => setSelected((current) => current.filter((id) => id !== item.id))}><span><strong>{String(item.display_name)}</strong><small>{String(item.username)} · {String(item.role)}</small></span><b>×</b></button>)}{added.length === 0 && <div className="association-empty">暂未添加用户</div>}</div></section>
      </div>
      {error && <div className="alert error">{error}</div>}
      <div className="modal-actions"><button type="button" onClick={onClose}>取消</button><button type="button" className="primary" disabled={busy} onClick={() => void save()}>{busy ? "保存中…" : "保存"}</button></div>
    </div>
  </div>;
}

function ResourceDialog({
  config,
  value,
  createFromCopy,
  onClose,
  onSaved,
}: {
  config: ResourceConfig;
  value: Resource | null;
  createFromCopy: boolean;
  onClose: () => void;
  onSaved: () => void;
}) {
  const createMode = !value || createFromCopy;
  const [form, setForm] = useState<Record<string, unknown>>(() =>
    Object.fromEntries(
      (config.fields ?? []).map((f) => [
        f.key,
        value?.[f.key] ??
          f.defaultValue ??
          (f.type === "checkbox" ? false : f.type === "multiselect" ? [] : ""),
      ]),
    ),
  );
  const [options, setOptions] = useState<Record<string, Resource[]>>({});
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    for (const f of config.fields ?? []) {
      if (f.options)
        void getPage<Resource>(f.options.endpoint, "", 1, {
          page_size: 100,
        }).then((r) => setOptions((o) => ({ ...o, [f.key]: r.items })));
    }
  }, [config.fields]);
  async function submit(e: FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError("");
    try {
      let payload = {
        ...form,
        ...(!createMode && value ? { version: value.version } : {}),
      };
      for (const field of config.fields ?? []) {
        const derived = field.choices?.find(
          (choice) => choice.value === form[field.key],
        )?.values;
        if (derived) payload = { ...payload, ...derived };
      }
      if (!createMode && value)
        await updateResource(config.endpoint, value.id, payload);
      else await createResource(config.endpoint, payload);
      onSaved();
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  }
  return (
    <div
      className="modal-backdrop"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <form className="modal" onSubmit={submit}>
        <div className="modal-head">
          <div>
            <p className="eyebrow">{createMode ? "CREATE" : "EDIT"}</p>
            <h2>
              {createMode ? "新建" : "编辑"}
              {config.title.replace("管理", "")}
            </h2>
          </div>
          <button type="button" onClick={onClose}>
            ×
          </button>
        </div>
        <div className="form-grid">
          {config.fields?.map((f) => (
            <FieldInput
              key={f.key}
              field={f}
              value={form[f.key]}
              options={options[f.key] ?? []}
              disabled={Boolean(!createMode && value && f.immutable)}
              onChange={(v) => setForm((x) => ({ ...x, [f.key]: v }))}
            />
          ))}
        </div>
        {error && <div className="alert error">{error}</div>}
        <div className="modal-actions">
          <button type="button" onClick={onClose}>
            取消
          </button>
          <button className="primary" disabled={busy}>
            {busy ? "保存中…" : "保存"}
          </button>
        </div>
      </form>
    </div>
  );
}

interface FolderItem {
  name: string;
  fullName: string;
  url: string;
  _class: string;
  jobs?: FolderItem[];
}
interface AssociationRow {
  folder: FolderItem;
  componentId: string;
  name: string;
}

function EnvironmentAssociationDialog({
  environment,
  onClose,
  onSaved,
}: {
  environment: Resource;
  onClose: () => void;
  onSaved: () => void;
}) {
  const navigate = useNavigate();
  const [folders, setFolders] = useState<FolderItem[]>([]);
  const [components, setComponents] = useState<Resource[]>([]);
  const [query, setQuery] = useState("");
  const [rows, setRows] = useState<AssociationRow[]>([]);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  useEffect(() => {
    Promise.all([
      http.get<FolderItem[]>(`/environments/${environment.id}/folders`),
      getPage<Resource>("/components", "", 1, { page_size: 100 }),
    ])
      .then(([folderResponse, componentPage]) => {
        setFolders(flattenFolders(folderResponse.data));
        setComponents(componentPage.items);
      })
      .catch((e) => setError(errorMessage(e)))
      .finally(() => setLoading(false));
  }, [environment.id]);
  const visibleFolders = folders.filter((folder) =>
    `${folder.name} ${folderFullName(folder)}`
      .toLowerCase()
      .includes(query.trim().toLowerCase()),
  );
  const componentLabel = (component: Resource) =>
    `${String(component.code)} · ${String(component.name)}`;
  const addFolder = (folder: FolderItem) =>
    setRows((current) =>
      current.some((row) => folderKey(row.folder) === folderKey(folder))
        ? current
        : [...current, { folder, componentId: "", name: folder.name }],
    );
  const updateRow = (index: number, patch: Partial<AssociationRow>) =>
    setRows((current) =>
      current.map((row, rowIndex) =>
        rowIndex === index ? { ...row, ...patch } : row,
      ),
    );
  const drop = (event: React.DragEvent) => {
    event.preventDefault();
    try {
      addFolder(
        JSON.parse(
          event.dataTransfer.getData("application/x-goodnight-folder"),
        ) as FolderItem,
      );
    } catch {
      /* ignore non-folder drops */
    }
  };
  async function submit(event: FormEvent) {
    event.preventDefault();
    if (rows.length === 0) {
      setError("请至少拖入一个 Jenkins Folder");
      return;
    }
    if (rows.some((row) => !row.componentId)) {
      setError("请为每个 Jenkins 目录选择组件");
      return;
    }
    if (rows.some((row) => !row.name.trim())) {
      setError("请填写实例名称");
      return;
    }
    setBusy(true);
    setError("");
    try {
      await Promise.all(
        rows.map((row) =>
          createResource("/component-instances", {
            name: row.name.trim(),
            component_id: row.componentId,
            environment_id: environment.id,
            folder_full_name: folderFullName(row.folder) || row.folder.name,
            folder_url: row.folder.url,
            notes: "",
            custom_fields: { folder_name: row.folder.name },
          }),
        ),
      );
      onSaved();
      navigate("/instances");
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <div
      className="modal-backdrop"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <form className="modal association-modal" onSubmit={submit}>
        <div className="modal-head">
          <div>
            <p className="eyebrow">ASSOCIATE COMPONENTS</p>
            <h2>
              关联组件 · {String(environment.customer_name)} /{" "}
              {String(environment.name)}
            </h2>
          </div>
          <button type="button" onClick={onClose}>
            ×
          </button>
        </div>
        {error && <div className="alert error">{error}</div>}
        <div className="association-layout">
          <section className="association-source">
            <header>
              <strong>Jenkins Folder</strong>
              <span>{folders.length} 个</span>
            </header>
            <div className="association-search">
              ⌕
              <input
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                placeholder="搜索 Folder 名称或路径…"
              />
            </div>
            <div className="folder-list">
              {loading ? (
                <div className="association-empty">正在读取 Jenkins…</div>
              ) : visibleFolders.length === 0 ? (
                <div className="association-empty">没有匹配的 Folder</div>
              ) : (
                visibleFolders.map((folder) => (
                  <div
                    className="folder-item"
                    draggable
                    key={folderKey(folder)}
                    onDragStart={(event) => {
                      event.dataTransfer.effectAllowed = "copy";
                      event.dataTransfer.setData(
                        "application/x-goodnight-folder",
                        JSON.stringify(folder),
                      );
                    }}
                  >
                    <span className="folder-grip">⋮⋮</span>
                    <a href={folder.url} target="_blank" rel="noreferrer">
                      {folder.name}
                    </a>
                    <button type="button" onClick={() => addFolder(folder)}>
                      添加
                    </button>
                  </div>
                ))
              )}
            </div>
          </section>
          <section
            className="association-target"
            onDragOver={(event) => {
              event.preventDefault();
              event.dataTransfer.dropEffect = "copy";
            }}
            onDrop={drop}
          >
            <header>
              <strong>组件实例</strong>
              <span>拖动左侧 Folder 到此处</span>
            </header>
            <div className="association-table-wrap">
              <table className="association-table">
                <thead>
                  <tr>
                    <th>Jenkins 目录</th>
                    <th>组件</th>
                    <th>实例名称</th>
                    <th />
                  </tr>
                </thead>
                <tbody>
                  {rows.length === 0 ? (
                    <tr>
                      <td colSpan={4} className="association-drop-empty">
                        将 Folder 拖到这里生成关联行
                      </td>
                    </tr>
                  ) : (
                    rows.map((row, index) => {
                      return (
                        <tr key={folderKey(row.folder)}>
                          <td>
                            <a
                              href={row.folder.url}
                              target="_blank"
                              rel="noreferrer"
                            >
                              {row.folder.name}
                            </a>
                            <small>{folderFullName(row.folder)}</small>
                          </td>
                          <td>
                            <SearchableSelect
                              ariaLabel="组件"
                              value={row.componentId}
                              placeholder="搜索并选择组件"
                              onChange={(componentId) =>
                                updateRow(index, { componentId })
                              }
                              options={components.map((component) => ({
                                value: component.id,
                                label: componentLabel(component),
                              }))}
                              required
                            />
                          </td>
                          <td>
                            <input
                              value={row.name}
                              onChange={(event) =>
                                updateRow(index, { name: event.target.value })
                              }
                              placeholder="请输入实例名称"
                              required
                            />
                          </td>
                          <td>
                            <button
                              type="button"
                              className="link-button danger-text"
                              onClick={() =>
                                setRows((current) =>
                                  current.filter(
                                    (_, rowIndex) => rowIndex !== index,
                                  ),
                                )
                              }
                            >
                              移除
                            </button>
                          </td>
                        </tr>
                      );
                    })
                  )}
                </tbody>
              </table>
            </div>
          </section>
        </div>
        <div className="modal-actions">
          <button type="button" onClick={onClose}>
            取消
          </button>
          <button className="primary" disabled={busy || loading}>
            {busy ? "保存中…" : `保存 ${rows.length || ""}`}
          </button>
        </div>
      </form>
    </div>
  );
}

function flattenFolders(items: FolderItem[]): FolderItem[] {
  return items.flatMap((item) => {
    const children = flattenFolders(item.jobs ?? []);
    return item._class.toLowerCase().includes("folder")
      ? [item, ...children]
      : children;
  });
}
function folderFullName(folder: FolderItem) {
  return folder.fullName || folder.name;
}
function folderKey(folder: FolderItem) {
  return folderFullName(folder) || folder.url;
}

interface JobItem {
  name: string;
  fullName: string;
  url: string;
  _class: string;
}
interface ParameterDefinition {
  name: string;
  type: string;
  description?: string;
  default?: unknown;
  choices?: unknown[];
}
interface JobPreview {
  job: JobItem;
  parameter_definitions: ParameterDefinition[];
}

function ManageJobsDialog({
  instance,
  onClose,
}: {
  instance: Resource;
  onClose: () => void;
}) {
  const navigate = useNavigate();
  const [jobs, setJobs] = useState<JobItem[]>([]);
  const [savedJobs, setSavedJobs] = useState<Record<string, Resource>>({});
  const [favorites, setFavorites] = useState<Set<string>>(new Set());
  const [selected, setSelected] = useState<JobItem>();
  const [query, setQuery] = useState("");
  const [previews, setPreviews] = useState<Record<string, JobPreview>>({});
  const [values, setValues] = useState<Record<string, Record<string, unknown>>>(
    {},
  );
  const [loading, setLoading] = useState(true);
  const [previewing, setPreviewing] = useState(false);
  const [busy, setBusy] = useState(false);
  const [testing, setTesting] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");

  useEffect(() => {
    Promise.all([
      http.get<JobItem[]>(`/component-instances/${instance.id}/jobs/discover`),
      getPage<Resource>("/job-configs", "", 1, { page_size: 100 }),
    ])
      .then(([jobResponse, jobPage]) => {
        const mine = jobPage.items.filter(
          (job) => job.component_instance_id === instance.id,
        );
        const byName = Object.fromEntries(
          mine.map((job) => [String(job.job_full_name), job]),
        );
        setJobs(jobResponse.data);
        setSavedJobs(byName);
        setFavorites(new Set(Object.keys(byName)));
      })
      .catch((e) => setError(errorMessage(e)))
      .finally(() => setLoading(false));
  }, [instance.id]);

  const visibleJobs = jobs.filter((job) =>
    `${job.name} ${job.fullName}`
      .toLowerCase()
      .includes(query.trim().toLowerCase()),
  );

  async function loadPreview(job: JobItem) {
    if (previews[job.fullName]) return previews[job.fullName];
    const preview = (
      await http.post<JobPreview>(
        `/component-instances/${instance.id}/jobs/preview`,
        { job_full_name: job.fullName },
      )
    ).data;
    setPreviews((current) => ({ ...current, [job.fullName]: preview }));
    const defaults = Object.fromEntries(
      preview.parameter_definitions.map((definition) => [
        definition.name,
        definition.default ?? (definition.type === "Boolean" ? false : ""),
      ]),
    );
    const saved = savedJobs[job.fullName]?.parameter_presets;
    setValues((current) => ({
      ...current,
      [job.fullName]: {
        ...defaults,
        ...(saved && typeof saved === "object"
          ? (saved as Record<string, unknown>)
          : {}),
      },
    }));
    return preview;
  }

  async function selectJob(job: JobItem) {
    setSelected(job);
    setError("");
    setNotice("");
    setPreviewing(true);
    try {
      await loadPreview(job);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setPreviewing(false);
    }
  }

  function toggleFavorite(job: JobItem) {
    setFavorites((current) => {
      const next = new Set(current);
      if (next.has(job.fullName)) next.delete(job.fullName);
      else next.add(job.fullName);
      return next;
    });
  }

  function updateParameter(name: string, value: unknown) {
    if (!selected) return;
    setValues((current) => ({
      ...current,
      [selected.fullName]: {
        ...(current[selected.fullName] ?? {}),
        [name]: value,
      },
    }));
  }

  async function testJob() {
    if (!selected) return;
    const popup = window.open("about:blank", "_blank");
    setTesting(true);
    setError("");
    setNotice("");
    try {
      await loadPreview(selected);
      const response = await http.post<{ location: string; queue_id: number }>(
        `/component-instances/${instance.id}/jobs/test`,
        {
          job_full_name: selected.fullName,
          parameters: values[selected.fullName] ?? {},
        },
      );
      setNotice("测试任务已提交，正在等待 Jenkins 分配执行器…");
      let executableUrl = "";
      for (let attempt = 0; attempt < 300; attempt += 1) {
        const queue = await http.post<{
          cancelled?: boolean;
          why?: string;
          executable_url?: string;
        }>(`/component-instances/${instance.id}/jobs/test/queue`, {
          queue_id: response.data.queue_id,
        });
        if (queue.data.cancelled) throw new Error("Jenkins 队列任务已取消");
        if (queue.data.executable_url) {
          executableUrl = queue.data.executable_url;
          break;
        }
        setNotice(
          queue.data.why
            ? `Jenkins 排队中：${queue.data.why}`
            : "Jenkins 排队中，等待可用执行器…",
        );
        await new Promise((resolve) => window.setTimeout(resolve, 2000));
      }
      if (!executableUrl) throw new Error("等待 Jenkins 开始执行超时");
      if (popup) popup.location.href = executableUrl;
      else window.open(executableUrl, "_blank", "noopener,noreferrer");
      setNotice("测试任务已开始执行，正在打开 Jenkins 构建页面");
    } catch (e) {
      popup?.close();
      setError(errorMessage(e));
    } finally {
      setTesting(false);
    }
  }

  async function save() {
    if (favorites.size === 0) {
      setError("请至少点亮一个 WorkflowJob");
      return;
    }
    setBusy(true);
    setError("");
    setNotice("");
    try {
      for (const job of jobs.filter((item) => favorites.has(item.fullName))) {
        let preview = previews[job.fullName];
        if (!preview) preview = await loadPreview(job);
        const defaults = Object.fromEntries(
          preview.parameter_definitions.map((definition) => [
            definition.name,
            definition.default ?? (definition.type === "Boolean" ? false : ""),
          ]),
        );
        const payload = {
          component_instance_id: instance.id,
          display_name: job.name,
          description: "",
          job_full_name: job.fullName,
          job_url: job.url,
          parameter_definitions: preview.parameter_definitions,
          parameter_presets: values[job.fullName] ?? defaults,
        };
        const saved = savedJobs[job.fullName];
        if (saved && previews[job.fullName])
          await updateResource("/job-configs", saved.id, {
            ...payload,
            version: saved.version,
          });
        else if (!saved) await createResource("/job-configs", payload);
      }
      navigate("/tasks/new");
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  const currentPreview = selected ? previews[selected.fullName] : undefined;
  const currentValues = selected ? (values[selected.fullName] ?? {}) : {};
  return (
    <div
      className="modal-backdrop"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div className="modal jobs-modal">
        <div className="modal-head">
          <div>
            <p className="eyebrow">MANAGE JENKINS JOBS</p>
            <h2>管理Job · {String(instance.name)}</h2>
          </div>
          <button type="button" onClick={onClose}>
            ×
          </button>
        </div>
        {error && <div className="alert error">{error}</div>}
        {notice && <div className="alert success-alert">{notice}</div>}
        <div className="jobs-layout">
          <section className="jobs-source">
            <header>
              <strong>WorkflowJob</strong>
              <span>{jobs.length} 个</span>
            </header>
            <div className="association-search">
              ⌕
              <input
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                placeholder="搜索 Job 名称或路径…"
              />
            </div>
            <div className="job-manage-list">
              {loading ? (
                <div className="association-empty">正在读取 Jenkins…</div>
              ) : visibleJobs.length === 0 ? (
                <div className="association-empty">没有匹配的 WorkflowJob</div>
              ) : (
                visibleJobs.map((job) => (
                  <div
                    key={job.fullName}
                    className={`job-manage-item ${selected?.fullName === job.fullName ? "selected" : ""}`}
                    onClick={() => void selectJob(job)}
                  >
                    <div>
                      <a
                        href={job.url}
                        target="_blank"
                        rel="noreferrer"
                        onClick={(event) => event.stopPropagation()}
                      >
                        {job.name}
                      </a>
                      <small>{job.fullName}</small>
                    </div>
                    <button
                      type="button"
                      className={`heart-button ${favorites.has(job.fullName) ? "favorite" : ""}`}
                      aria-label={
                        favorites.has(job.fullName)
                          ? `取消关注 ${job.name}`
                          : `关注 ${job.name}`
                      }
                      onClick={(event) => {
                        event.stopPropagation();
                        toggleFavorite(job);
                      }}
                    >
                      ♥
                    </button>
                  </div>
                ))
              )}
            </div>
          </section>
          <section className="job-preview">
            <header>
              <strong>参数预览</strong>
              {selected && (
                <a href={selected.url} target="_blank" rel="noreferrer">
                  {selected.name} ↗
                </a>
              )}
            </header>
            {!selected ? (
              <div className="job-preview-empty">
                点击左侧 WorkflowJob 查看运行参数
              </div>
            ) : previewing ? (
              <div className="job-preview-empty">正在读取参数定义…</div>
            ) : currentPreview ? (
              <div className="parameter-form">
                {currentPreview.parameter_definitions.length === 0 ? (
                  <div className="job-preview-empty">该 Job 无运行参数</div>
                ) : (
                  currentPreview.parameter_definitions.map((definition) => (
                    <ParameterInput
                      key={definition.name}
                      definition={definition}
                      value={currentValues[definition.name]}
                      onChange={(value) =>
                        updateParameter(definition.name, value)
                      }
                    />
                  ))
                )}
              </div>
            ) : (
              <div className="job-preview-empty">参数读取失败，请重新选择</div>
            )}
          </section>
        </div>
        <div className="modal-actions">
          <button type="button" onClick={onClose}>
            取消
          </button>
          <button
            type="button"
            className="debug-button"
            disabled={!selected || previewing || testing}
            onClick={() => void testJob()}
          >
            <span className="debug-icon" aria-hidden="true">
              🐞
            </span>
            {testing ? "提交中…" : "测试"}
          </button>
          <button
            type="button"
            className="primary"
            disabled={busy || loading}
            onClick={() => void save()}
          >
            {busy ? "保存中…" : `保存Job (${favorites.size})`}
          </button>
        </div>
      </div>
    </div>
  );
}

function ParameterInput({
  definition,
  value,
  onChange,
}: {
  definition: ParameterDefinition;
  value: unknown;
  onChange: (value: unknown) => void;
}) {
  const description = definition.description && (
    <small>{definition.description}</small>
  );
  if (definition.type === "Boolean")
    return (
      <label className="parameter-checkbox">
        <span>
          <strong>{definition.name}</strong>
          {description}
        </span>
        <input
          type="checkbox"
          checked={Boolean(value)}
          onChange={(event) => onChange(event.target.checked)}
        />
      </label>
    );
  if (definition.type === "Choice")
    return (
      <label>
        <strong>{definition.name}</strong>
        <select
          value={String(value ?? "")}
          onChange={(event) => onChange(event.target.value)}
        >
          {(definition.choices ?? []).map((choice) => (
            <option key={String(choice)} value={String(choice)}>
              {String(choice)}
            </option>
          ))}
        </select>
        {description}
      </label>
    );
  if (definition.type === "Text")
    return (
      <label>
        <strong>{definition.name}</strong>
        <textarea
          value={String(value ?? "")}
          onChange={(event) => onChange(event.target.value)}
        />
        {description}
      </label>
    );
  return (
    <label>
      <strong>{definition.name}</strong>
      <input
        type={definition.type === "Password" ? "password" : "text"}
        value={String(value ?? "")}
        onChange={(event) => onChange(event.target.value)}
      />
      {description}
    </label>
  );
}

function FieldInput({
  field,
  value,
  options,
  disabled,
  onChange,
}: {
  field: Field;
  value: unknown;
  options: Resource[];
  disabled: boolean;
  onChange: (v: unknown) => void;
}) {
  const klass = field.full ? "full" : undefined;
  if (field.type === "checkbox")
    return (
      <label className="checkbox-field">
        <input
          type="checkbox"
          checked={Boolean(value)}
          disabled={disabled}
          onChange={(e) => onChange(e.target.checked)}
        />
        <span>
          <strong>{field.label}</strong>
          <small>{field.placeholder}</small>
        </span>
      </label>
    );
  if (field.type === "select")
    if (isSearchableRelation(field.key))
      return (
        <label className={klass}>
          {field.label}
          <SearchableSelect
            ariaLabel={field.label}
            value={String(value ?? "")}
            placeholder={`搜索并选择${field.label}`}
            disabled={disabled}
            required={field.required}
            onChange={onChange}
            options={options.map((option) => ({
              value: option.id,
              label: field.options?.label(option) ?? option.id,
            }))}
          />
        </label>
      );
  if (field.type === "select")
    return (
      <label className={klass}>
        {field.label}
        <select
          value={String(value)}
          required={field.required}
          disabled={disabled}
          onChange={(e) => onChange(e.target.value)}
        >
          <option value="">请选择</option>
          {field.choices?.map((o) => (
            <option key={o.value} value={o.value}>
              {o.label}
            </option>
          ))}
          {options.map((o) => (
            <option key={o.id} value={o.id}>
              {field.options?.label(o)}
            </option>
          ))}
        </select>
      </label>
    );
  if (field.type === "multiselect")
    return (
      <label className="full">
        {field.label}
        <select
          multiple
          value={value as string[]}
          required={field.required}
          disabled={disabled}
          onChange={(e) =>
            onChange([...e.target.selectedOptions].map((o) => o.value))
          }
        >
          {options.map((o) => (
            <option key={o.id} value={o.id}>
              {field.options?.label(o)}
            </option>
          ))}
        </select>
        <small>按住 Ctrl/Command 可多选</small>
      </label>
    );
  if (field.type === "textarea" || field.type === "json")
    return (
      <label className="full">
        {field.label}
        <textarea
          value={
            typeof value === "string" ? value : JSON.stringify(value, null, 2)
          }
          required={field.required}
          disabled={disabled}
          placeholder={field.placeholder}
          onChange={(e) => {
            if (field.type === "json") {
              try {
                onChange(JSON.parse(e.target.value));
              } catch {
                onChange(e.target.value);
              }
            } else onChange(e.target.value);
          }}
        />
      </label>
    );
  return (
    <label className={klass}>
      {field.label}
      <input
        type={field.type ?? "text"}
        value={String(value ?? "")}
        required={field.required}
        disabled={disabled}
        placeholder={field.placeholder}
        onChange={(e) =>
          onChange(
            field.type === "number" ? Number(e.target.value) : e.target.value,
          )
        }
      />
    </label>
  );
}
function render(value: unknown) {
  if (typeof value === "boolean")
    return (
      <span className={`status ${value ? "success" : "muted"}`}>
        {value ? "启用" : "禁用"}
      </span>
    );
  if (value == null || value === "") return <span className="dash">—</span>;
  if (typeof value === "object") return <code>{JSON.stringify(value)}</code>;
  return String(value);
}
export const status = (value: unknown) =>
  typeof value === "boolean" ? (
    render(value)
  ) : (
    <span className={`status ${String(value).toLowerCase()}`}>
      {String(value)}
    </span>
  );

function isSearchableRelation(key: string) {
  return key === "component_id" || key === "customer_id" || key === "owner_id";
}
