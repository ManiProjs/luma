import { defineConfig } from 'vitepress'
export default defineConfig({
 title:'Luma',description:'Documentation for the Luma agent workspace.',base:'/luma/docs/',outDir:'../dist/docs',cleanUrls:true,appearance:'dark',
 head:[['link',{rel:'icon',type:'image/svg+xml',href:'luma-mark.svg'}]],
 themeConfig:{logo:'luma-mark.svg',siteTitle:'LUMA',nav:[{text:'Guide',link:'/architecture'},{text:'Teammates',link:'/teammates'},{text:'GitHub',link:'https://github.com/ManiProjs/luma'}],
 sidebar:[{text:'Getting started',items:[{text:'Architecture',link:'/architecture'},{text:'Configuration & models',link:'/configuration'},{text:'CLI',link:'/cli'}]},{text:'Agent workspace',items:[{text:'Teammates',link:'/teammates'},{text:'Tools & approvals',link:'/tools'},{text:'Agent Skills',link:'/'+'skills'}]},{text:'Integrations',items:[{text:'Native plugins',link:'/plugins'},{text:'MCP',link:'/mcp'}]}],
 socialLinks:[{icon:'github',link:'https://github.com/ManiProjs/luma'}],footer:{message:'Open source. Built in Rust.',copyright:'© Luma'},search:{provider:'local'},editLink:{pattern:'https://github.com/ManiProjs/luma/edit/main/docs/:path',text:'Edit this page on GitHub'}}
})
