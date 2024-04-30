{-# LANGUAGE LambdaCase #-}
{-# LANGUAGE PatternSynonyms #-}
{-# LANGUAGE ViewPatterns #-}

module Website.AjaxGoogleApis.Html (getPackages) where

import Control.Monad
import qualified Data.List as L
import qualified Data.Map as M
import Data.Maybe
import Data.Text (Text)
import qualified Data.Text as T
import Website.AjaxGoogleApis.Types
import Zenacy.HTML

findArticle :: HTMLNode -> Maybe [HTMLNode]
findArticle HTMLText {} = Nothing
findArticle HTMLComment {} = Nothing
findArticle HTMLElement {htmlElementName = "article", htmlElementChildren = ch} = Just ch
findArticle HTMLElement {htmlElementChildren = ch} =
  listToMaybe (mapMaybe findArticle ch)
findArticle HTMLDocument {htmlDocumentChildren = ch} =
  listToMaybe (mapMaybe findArticle ch)
findArticle HTMLFragment {} = Nothing
findArticle HTMLDoctype {} = Nothing
findArticle HTMLTemplate {} = Nothing

pattern Element :: Text -> [HTMLAttr] -> [HTMLNode] -> HTMLNode
pattern Element n a c <- HTMLElement {htmlElementName = n, htmlElementAttributes = a, htmlElementChildren = c}

pattern Attr :: Text -> Text -> HTMLAttr
pattern Attr n v <- HTMLAttr {htmlAttrName = n, htmlAttrVal = v}

snippets :: Maybe [Text] -> M.Map Text [Text] -> [HTMLNode] -> (Maybe [Text], M.Map Text [Text], [HTMLNode])
snippets
  unnamed
  mp
  ( Element "dt" _ [HTMLText (T.unsnoc >=> T.stripSuffix "snippet" . fst -> Just n)] -- snippet[:>]
      : Element "dd" _ ch
      : other
    ) =
    let sn = T.strip n
        s =
          fmap
            ( \case
                Element "code" _ c ->
                  T.unlines
                    ( mapMaybe
                        ( \case
                            HTMLText t -> Just (T.strip t)
                            HTMLElement {htmlElementName = "br"} -> Nothing
                            _ -> error ("Unknown node in snippet code: " ++ show c)
                        )
                        c
                    )
                _ -> error ("Unknown node in snippet: " ++ show ch)
            )
            ch
     in if T.null sn
          then snippets (Just s) mp other
          else snippets unnamed (M.insert sn s mp) other
snippets s mp ns = (s, mp, ns)

versions :: Maybe [Text] -> M.Map Text [Text] -> [HTMLNode] -> (Maybe [Text], M.Map Text [Text], [HTMLNode])
versions unnamed mp (Element "dt" _ [HTMLText (T.stripSuffix "versions:" -> Just n)] : o) =
  let vn = T.strip n
      (vt, rest) = versionN [] o
   in if T.null vn
        then versions (Just vt) mp rest
        else versions unnamed (M.insert vn vt mp) rest
  where
    versionN vt (Element "dd" _ v : rest) =
      versionN
        ( concatMap
            ( \case
                HTMLText t ->
                  (filter (not . T.null) . map T.strip . T.split (== ',')) t
                Element "br" _ _ -> []
                _ -> error ("unknown node in version: " ++ show v)
            )
            v
            ++ vt
        )
        rest
    versionN vt rest = (vt, rest)
versions u n s = (u, n, s)

packageId :: Text -> PackageId
packageId i = case i of
  "cesiumjs" -> PCesium
  "d3.js" -> PD3
  "dojo" -> PDojo
  "ext-core" -> PExtCore
  "hammer.js" -> PHammer
  "indefinite-observable" -> PIndefiniteObservable
  "jquery" -> PJQuery
  "jquery-mobile" -> PJQueryMobile
  "jquery-ui" -> PJQueryUI
  "list.js" -> PList
  "material-motion" -> PMaterialMotion
  "model-viewer" -> PModelViewer
  "mootools" -> PMootools
  "myanmar-tools" -> PMyanmarTools
  "prototype" -> PPrototype
  "script.aculo.us" -> PScriptAculoUs
  "shaka-player" -> PShakaPlayer
  "spf" -> PSpf
  "swfobject" -> PSwfobject
  "three.js" -> PThree
  "web-font-loader" -> PWebFontLoader
  _ -> PUnknown i

getLibraries :: [HTMLNode] -> [Package]
getLibraries [] = []
getLibraries
  ( Element "h3" (Attr "id" i : Attr "data-text" name : _) _
      : Element
          "dl"
          _
          ch
      : other
    ) =
    ( let (us, ns, rs) = snippets Nothing M.empty ch
       in case rs of
            Element "dt" _ [HTMLText "site:"]
              : Element "dd" _ [Element "a" [Attr "href" link] _]
              : sr ->
                let (uv, nv, _) = versions Nothing M.empty sr
                 in Package
                      { pkgId = packageId i,
                        pkgElementId = i,
                        pkgName = name,
                        pkgSnippets = us,
                        pkgNamedSnippets = ns,
                        pkgWebsite = link,
                        pkgVersions = uv,
                        pkgNamedVersions = nv
                      }
            _ -> error ("Unknown library node: " ++ show ch)
    )
      : getLibraries other
getLibraries (Element "h2" (Attr "id" "troubleshooting" : _) _ : _) = []
getLibraries ns = error ("Unknown node: " ++ show ns)

getPackages :: HTMLNode -> [Package]
getPackages hn =
  case mapMaybe htmlSpaceRemove <$> findArticle hn of
    Just [Element "div" _ _, Element "devsite-toc" _ _, Element "div" _ ch, _] ->
      getLibraries
        ( L.dropWhile
            ( \case
                Element "h3" _ _ -> False
                _ -> True
            )
            ch
        )
    Just n -> error ("unknown article: " ++ show n)
    Nothing -> error "Unknown html"