import { ResourcePage,type ResourceConfig } from '../components/ResourcePage'
const config:ResourceConfig={title:'审计日志',description:'关键配置和任务操作的不可变审计轨迹。',endpoint:'/audit-logs',columns:[{key:'created_at',label:'时间',render:v=>new Date(String(v)).toLocaleString()},{key:'actor_name',label:'操作者'},{key:'action',label:'操作'},{key:'object_type',label:'对象类型'},{key:'object_id',label:'对象 ID'},{key:'summary',label:'变更摘要'}]}
export const AuditPage=()=> <ResourcePage config={config}/>
