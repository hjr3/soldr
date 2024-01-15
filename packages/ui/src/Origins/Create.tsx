import { Create, SimpleForm, NumberInput, TextInput, required } from 'react-admin';

export const OriginsCreate = () => (
  <Create>
    <SimpleForm>
      <TextInput source="domain" validate={[required()]} />
      <TextInput source="origin_uri" validate={[required()]} />
      <NumberInput source="timeout" defaultValue={100} validate={[required()]} />
    </SimpleForm>
  </Create>
);

export default OriginsCreate;
