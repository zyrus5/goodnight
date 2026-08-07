import { useEffect } from 'react'
import { Navigate, NavLink, Outlet, useLocation } from 'react-router-dom'
import { useAppStore } from '../stores/app'

const navigation=[
  ['/', '总览', '⌂'],['/components','组件','◇'],['/customers','客户','◎'],['/environments','环境','◉'],
  ['/instances','组件实例','▦'],['/tasks','任务中心','⌘'],['/executions','执行记录','◷'],
] as const

export function AppLayout(){
  const {user,ready,loadUser,logout}=useAppStore();const location=useLocation()
  const currentNavigation=navigation.find(([to])=>to==='/'?location.pathname==='/':location.pathname===to||location.pathname.startsWith(`${to}/`))
  useEffect(()=>{if(!ready)void loadUser()},[ready,loadUser])
  if(!ready)return <div className="screen-center"><div className="spinner"/>正在加载平台…</div>
  if(!user)return <Navigate to="/login" state={{from:location}} replace/>
  return <div className="app-shell">
    <aside className="sidebar"><div className="brand"><span className="brand-mark">G</span><div><strong>Goodnight</strong><small>Jenkins 编排平台</small></div></div>
      <nav>{navigation.map(([to,label,icon])=><NavLink key={to} to={to} end={to==='/' }><span>{icon}</span>{label}</NavLink>)}</nav>
      {user.is_admin&&<div className="nav-section"><small>系统管理</small><NavLink to="/users"><span>♙</span>用户管理</NavLink><NavLink to="/audit"><span>≡</span>审计日志</NavLink></div>}
      <div className="sidebar-user"><div className="avatar">{user.display_name.slice(0,1)}</div><div><strong>{user.display_name}</strong><small>{{ADMIN:'系统管理员',OPS:'运维',DEVELOPER:'开发',TESTER:'测试'}[user.role]??user.role}</small></div><button title="退出" onClick={()=>void logout()}>↪</button></div>
    </aside>
    <main className="workspace"><header className="topbar"><div><span className="crumb">统一发布控制台</span><strong>{currentNavigation?.[1]??'平台管理'}</strong></div><div className="live"><i/>服务运行中</div></header><div className="content"><Outlet/></div></main>
  </div>
}
