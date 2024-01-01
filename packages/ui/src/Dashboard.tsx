import Card from '@mui/material/Card';
import CardContent from '@mui/material/CardContent';
import { Title } from 'react-admin';

const Dashboard = () => (
  <Card>
    <Title title="Soldr - Management UI" />
    <CardContent>
      Use this UI to:
      <ul>
        <li>Create and manage your Origins</li>
        <li>Review failing requests</li>
        <li>Diagnose issues by reviewing failed request attempts</li>
        <li>Modify a failing request so it succeeds</li>
      </ul>
    </CardContent>
  </Card>
);

export default Dashboard;
