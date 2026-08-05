import { useEffect, useState } from 'react'

import { getHealth, type HealthResponse } from '../services/system'
import { useAppStore } from '../stores/app'

export function HomePage() {
  const visits = useAppStore((state) => state.visits)
  const incrementVisits = useAppStore((state) => state.incrementVisits)
  const [health, setHealth] = useState<HealthResponse | null>(null)

  useEffect(() => {
    incrementVisits()
    void getHealth().then(setHealth).catch(() => setHealth(null))
  }, [incrementVisits])

  return (
    <section className="hero">
      <p className="eyebrow">React + Axum + PostgreSQL</p>
      <h1>工程脚手架已启动</h1>
      <p>路由、状态管理与 HTTP 客户端均已就绪。</p>
      <div className="status-grid">
        <div>
          <span>前端访问次数</span>
          <strong>{visits}</strong>
        </div>
        <div>
          <span>API / 数据库</span>
          <strong>
            {health ? `${health.service} / ${health.database}` : '未连接'}
          </strong>
        </div>
      </div>
    </section>
  )
}
