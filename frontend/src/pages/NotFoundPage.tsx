import { Link } from 'react-router-dom'

export function NotFoundPage() {
  return (
    <section className="not-found">
      <p className="eyebrow">404</p>
      <h1>页面不存在</h1>
      <Link to="/">返回首页</Link>
    </section>
  )
}
