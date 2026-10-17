// App source consumed by the real source publication API, never registered in Core.
export function unknownApp(id) {
 const collection=id.replaceAll('-','_')+'_items';
 const schema={version:0,primaryKey:'id',type:'object',properties:{id:{type:'string'},title:{type:'string'},kind:{type:'string'}},required:['id','title'],indexes:['title']};
 const ui=`export function mount(root,api,ctx={}){
 const input=document.createElement('input');input.value=ctx.restored?.input||'';input.dataset.k24='input';
 const list=document.createElement('ul');const off=api.query('${collection}',{selector:{}}).subscribe(rows=>{list.replaceChildren(...rows.map(d=>{const li=document.createElement('li');li.dataset.id=d.id;li.textContent=d.title??d.label;return li;}));});
 const button=document.createElement('button');button.dataset.k24='command';button.textContent='Write via handler';button.onclick=()=>{const h=api.command('bench.write',{id:'handler-row',title:'handler result'});h.local.catch(e=>api.status.report({level:'error',code:e.code,text:e.message}));};
 root.replaceChildren(input,button,list);return{unmount(){off();root.replaceChildren();},snapshot(){return{input:input.value};}};}`;
 const handler=`export default async function(command,ctx){await ctx.cap.config.get({key:'locale'});await ctx.put('${collection}',{id:command.payload.id,title:command.payload.title,kind:'handler'});return{written:command.payload.id};}`;
 const tick=`export default async function(command,ctx){await ctx.put('${collection}',{id:'tick-row',title:'scheduled',kind:'trigger'});}`;
 return {id,collection,schema,files:{'index.mjs':ui,'handlers/write.mjs':handler,'handlers/tick.mjs':tick},
  manifest:{title:'Unknown E2E app',entry:'index.mjs',handlers:{'bench.write':'handlers/write.mjs'},triggers:[{handler:'handlers/tick.mjs',every:'PT1M'}],capabilities_required:['config.get'],command_rights:{'bench.write':{roles:['admin','user']}}}};
}
