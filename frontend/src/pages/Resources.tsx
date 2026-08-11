import {
  ResourcePage,
  status,
  type ResourceConfig,
} from "../components/ResourcePage";

const active = { key: "is_active", label: "状态", render: status };
const date = {
  key: "updated_at",
  label: "更新时间",
  render: (v: unknown) => (v ? new Date(String(v)).toLocaleString() : "—"),
};
const users: ResourceConfig = {
  title: "用户管理",
  description: "维护本地账户、管理员身份和启停状态。",
  endpoint: "/users",
  adminOnly: true,
  columns: [
    { key: "username", label: "用户名" },
    { key: "display_name", label: "姓名" },
    {
      key: "role",
      label: "角色",
      render: (v) => ({ ADMIN: "系统管理员", OPS: "运维", DEVELOPER: "开发", TESTER: "测试" })[String(v)] ?? String(v),
    },
    active,
    date,
  ],
  fields: [
    { key: "username", label: "用户名", required: true, immutable: true },
    { key: "display_name", label: "显示名称", required: true },
    {
      key: "password",
      label: "密码",
      type: "password",
      placeholder: "更新时留空表示不修改",
    },
    { key: "role", label: "系统角色", type: "select", required: true, defaultValue: "TESTER", choices: [
      { value: "ADMIN", label: "系统管理员" },
      { value: "OPS", label: "运维" },
      { value: "DEVELOPER", label: "开发" },
      { value: "TESTER", label: "测试" },
    ] },
    { key: "is_active", label: "启用账户", type: "checkbox" },
  ],
};
const customers: ResourceConfig = {
  title: "客户管理",
  description: "维护交付客户及其环境入口。",
  endpoint: "/customers",
  maintainerOnly: true,
  columns: [
    { key: "code", label: "客户代码" },
    { key: "name", label: "客户名称" },
    { key: "environment_count", label: "环境数量" },
    active,
    date,
  ],
  fields: [
    { key: "code", label: "客户代码", required: true, immutable: true },
    { key: "name", label: "客户名称", required: true },
    {
      key: "is_active",
      label: "启用客户",
      type: "checkbox",
      defaultValue: true,
    },
  ],
};
const components: ResourceConfig = {
  title: "组件管理",
  description: "管理业务组件负责人、开发人员和测试人员。",
  endpoint: "/components",
  maintainerOnly: true,
  memberRoles: [
    { role: "DEVELOPER", label: "添加开发人员", field: "developer_ids" },
    { role: "TESTER", label: "添加测试人员", field: "tester_ids" },
  ],
  columns: [
    { key: "code", label: "组件代码" },
    { key: "name", label: "组件名称" },
    { key: "owner_names", label: "组件负责人" },
    { key: "developer_names", label: "开发人员" },
    { key: "tester_names", label: "测试人员" },
    { key: "is_public", label: "公共组件", render: (v) => v ? "是" : "否" },
    { key: "instance_count", label: "实例数" },
    active,
    date,
  ],
  fields: [
    { key: "code", label: "组件代码", required: true, immutable: true },
    { key: "name", label: "组件名称", required: true },
    {
      key: "owner_id",
      label: "组件负责人",
      type: "select",
      required: true,
      options: { endpoint: "/users", label: (r) => String(r.display_name) },
    },
    {
      key: "is_public",
      label: "是否公共组件",
      type: "checkbox",
      defaultValue: false,
    },
    {
      key: "is_active",
      label: "启用组件",
      type: "checkbox",
      defaultValue: true,
    },
  ],
};
const environmentChoices = [
  { value: "dev", label: "dev - 开发", values: { name: "开发" } },
  { value: "test", label: "test - 测试", values: { name: "测试" } },
  { value: "uat", label: "uat - 验收", values: { name: "验收" } },
  { value: "branch", label: "branch - 分支", values: { name: "分支" } },
  { value: "prod", label: "prod - 生产", values: { name: "生产" } },
];
const environments: ResourceConfig = {
  title: "环境管理",
  description:
    "配置 Jenkins 连接；创建时自动检测，连接成功则启用，失败则停用。",
  endpoint: "/environments",
  maintainerOnly: true,
  copyable: true,
  associateComponents: true,
  filters: [
    {
      key: "customer_id",
      label: "客户",
      type: "select",
      options: {
        endpoint: "/customers",
        label: (r) => `${r.code} · ${r.name}`,
      },
    },
    { key: "deployment_domain", label: "部署域", placeholder: "筛选部署域" },
  ],
  columns: [
    { key: "customer_name", label: "客户" },
    { key: "deployment_domain", label: "部署域" },
    { key: "code", label: "环境代码" },
    { key: "name", label: "环境名称" },
    { key: "jenkins_url", label: "Jenkins 地址" },
    active,
    date,
  ],
  fields: [
    {
      key: "customer_id",
      label: "客户",
      type: "select",
      required: true,
      options: {
        endpoint: "/customers",
        label: (r) => `${r.code} · ${r.name}`,
      },
    },
    { key: "deployment_domain", label: "部署域" },
    {
      key: "code",
      label: "环境代码 - 环境名称",
      type: "select",
      choices: environmentChoices,
      required: true,
      immutable: true,
    },
    { key: "jenkins_url", label: "Jenkins 根地址", required: true, full: true },
    {
      key: "request_timeout_seconds",
      label: "请求超时（秒）",
      type: "number",
      defaultValue: 10,
    },
    { key: "notes", label: "备注", type: "textarea" },
  ],
};
const instances: ResourceConfig = {
  title: "组件实例",
  description: "组件实例统一通过环境管理的“关联组件”功能创建。",
  endpoint: "/component-instances",
  createDisabled: true,
  maintainerOnly: true,
  filters: [
    {
      key: "component_id",
      label: "组件",
      type: "select",
      options: {
        endpoint: "/components",
        label: (r) => `${r.code} · ${r.name}`,
      },
    },
    {
      key: "customer_id",
      label: "客户",
      type: "select",
      options: {
        endpoint: "/customers",
        label: (r) => `${r.code} · ${r.name}`,
      },
    },
    {
      key: "environment_id",
      label: "环境",
      type: "select",
      options: {
        endpoint: "/environments",
        label: (r) => `${r.customer_name} / ${r.name}`,
      },
    },
    { key: "deployment_domain", label: "部署域", placeholder: "筛选部署域" },
  ],
  columns: [
    { key: "name", label: "实例名称" },
    { key: "component_name", label: "组件" },
    { key: "customer_name", label: "客户" },
    { key: "environment_name", label: "环境" },
    { key: "deployment_domain", label: "部署域" },
    { key: "wiki_url", label: "Wiki", render: externalLink("打开 Wiki") },
    { key: "argo_url", label: "Argo", render: externalLink("打开 Argo") },
    { key: "apollo_url", label: "Apollo", render: externalLink("打开 Apollo") },
    { key: "log_url", label: "日志地址", render: externalLink("打开日志") },
    {
      key: "folder_full_name",
      label: "Folder 完整路径",
      render: (value, row) => {
        const custom = row.custom_fields as
          { folder_name?: string } | undefined;
        const label =
          custom?.folder_name ??
          String(value).split("/").filter(Boolean).at(-1) ??
          String(value);
        return row.folder_url ? (
          <a href={String(row.folder_url)} target="_blank" rel="noreferrer">
            {label}
          </a>
        ) : (
          label
        );
      },
    },
    { key: "status", label: "状态", render: status },
    date,
  ],
  fields: [
    { key: "name", label: "实例名称", required: true },
    { key: "wiki_url", label: "Wiki 地址", full: true },
    { key: "argo_url", label: "Argo 地址", full: true },
    { key: "apollo_url", label: "Apollo 地址", full: true },
    { key: "log_url", label: "日志地址", full: true },
  ],
};
instances.deletable = true;
instances.manageJobs = true;

function externalLink(label: string) {
  return (value: unknown) => value ? (
    <a href={String(value)} target="_blank" rel="noreferrer">{label}</a>
  ) : "—";
}
export const UsersPage = () => <ResourcePage config={users} />;
export const CustomersPage = () => <ResourcePage config={customers} />;
export const ComponentsPage = () => <ResourcePage config={components} />;
export const EnvironmentsPage = () => <ResourcePage config={environments} />;
export const InstancesPage = () => <ResourcePage config={instances} />;
