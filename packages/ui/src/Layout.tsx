import { CheckForApplicationUpdate, Layout as RaLayout, LayoutProps } from 'react-admin';

const Layout = ({ children, ...props }: LayoutProps) => (
  <RaLayout {...props}>
    {children}
    <CheckForApplicationUpdate />
  </RaLayout>
);

export default Layout;
