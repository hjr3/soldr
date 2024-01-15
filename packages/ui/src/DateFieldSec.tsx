import { DateField, DateFieldProps } from 'react-admin';

const DateFieldSec = (props: DateFieldProps) => (
  <DateField transform={(value: number) => new Date(value * 1000)} {...props} />
);

export default DateFieldSec;
