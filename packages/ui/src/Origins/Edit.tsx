import { Edit, SimpleForm, NumberInput, TextInput, required } from 'react-admin';

export const OriginsEdit = () => (
  <Edit>
    <SimpleForm>
      <TextInput disabled label="Id" source="id" />
      <TextInput source="domain" validate={[required()]} />
      <TextInput source="origin_uri" validate={[required()]} />
      <NumberInput source="timeout" defaultValue="100" validate={[required()]} />
    </SimpleForm>
  </Edit>
);

export default OriginsEdit;
