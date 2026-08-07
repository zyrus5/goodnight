import { createBrowserRouter } from 'react-router-dom'
import { AppLayout } from '../components/AppLayout'
import { AuditPage } from '../pages/AuditPage'
import { ExecutionDetailPage } from '../pages/ExecutionDetailPage'
import { ExecutionsPage } from '../pages/ExecutionsPage'
import { HomePage } from '../pages/HomePage'
import { LoginPage } from '../pages/LoginPage'
import { NotFoundPage } from '../pages/NotFoundPage'
import { ComponentsPage,CustomersPage,EnvironmentsPage,InstancesPage,UsersPage } from '../pages/Resources'
import { TaskEditorPage } from '../pages/TaskEditorPage'
import { TasksPage } from '../pages/TasksPage'

export const router=createBrowserRouter([
  {path:'/login',element:<LoginPage/>},
  {path:'/',element:<AppLayout/>,children:[
    {index:true,element:<HomePage/>},{path:'components',element:<ComponentsPage/>},{path:'customers',element:<CustomersPage/>},
    {path:'environments',element:<EnvironmentsPage/>},{path:'instances',element:<InstancesPage/>},
    {path:'tasks',element:<TasksPage/>},{path:'tasks/new',element:<TaskEditorPage/>},{path:'tasks/:id',element:<TaskEditorPage/>},
    {path:'executions',element:<ExecutionsPage/>},{path:'executions/:id',element:<ExecutionDetailPage/>},
    {path:'users',element:<UsersPage/>},{path:'audit',element:<AuditPage/>},{path:'*',element:<NotFoundPage/>},
  ]},
])
