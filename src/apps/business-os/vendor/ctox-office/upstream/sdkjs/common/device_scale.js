/*
 * Copyright (C) Ascensio System SIA 2012-2026. All rights reserved
 *
 * https://www.onlyoffice.com/
 *
 * Version: 0.0.0 (build:0)
 */

(function(window,undefined){var supportedScaleValues=[1,1.25,1.5,1.75,2,2.25,2.5,2.75,3,3.5,4,4.5,5];if(window["AscDesktopEditor"]&&window["AscDesktopEditor"]["GetSupportedScaleValues"])supportedScaleValues=window["AscDesktopEditor"]["GetSupportedScaleValues"]();var isCorrectApplicationScaleEnabled=function(){if(supportedScaleValues.length===0)return false;var userAgent=navigator.userAgent.toLowerCase();var isAndroid=userAgent.indexOf("android")>-1;var isIE=userAgent.indexOf("msie")>-1||userAgent.indexOf("trident")>
-1||userAgent.indexOf("edge")>-1;var isChrome=!isIE&&userAgent.indexOf("chrome")>-1;var isOperaOld=!!window.opera;var isMobile=/android|avantgo|blackberry|blazer|compal|elaine|fennec|hiptop|iemobile|ip(hone|od|ad)|iris|kindle|lge |maemo|midp|mmp|opera m(ob|in)i|palm( os)?|phone|p(ixi|re)\/|plucker|pocket|psp|symbian|treo|up\.(browser|link)|vodafone|wap|windows (ce|phone)|xda|xiino/i.test(navigator.userAgent||navigator.vendor||window.opera);if(isAndroid||!isChrome||isOperaOld||isMobile||!document||
!document.firstElementChild||!document.body)return false;return true}();window["AscCommon"]=window["AscCommon"]||{};window["AscCommon"].checkDeviceScale=function(){var retValue={zoom:1,devicePixelRatio:window.devicePixelRatio,applicationPixelRatio:window.devicePixelRatio,correct:false};if(!isCorrectApplicationScaleEnabled)return retValue;var systemScaling=window.devicePixelRatio;var bestIndex=0;var bestDistance=Math.abs(supportedScaleValues[0]-systemScaling);var currentDistance=0;var i=1;var len=
supportedScaleValues.length;for(;i<len;i++){if(true)if(Math.abs(supportedScaleValues[i]-systemScaling)>1E-4)if(supportedScaleValues[i]>systemScaling-1E-4)break;currentDistance=Math.abs(supportedScaleValues[i]-systemScaling);if(currentDistance<bestDistance-1E-4){bestDistance=currentDistance;bestIndex=i}}retValue.applicationPixelRatio=supportedScaleValues[bestIndex];if(Math.abs(retValue.devicePixelRatio-retValue.applicationPixelRatio)>.01){retValue.zoom=retValue.devicePixelRatio/retValue.applicationPixelRatio;
retValue.correct=true}return retValue};var oldZoomValue=1;window["AscCommon"].correctApplicationScale=function(zoomValue){if(!zoomValue.correct&&Math.abs(zoomValue.zoom-oldZoomValue)<1E-4)return;oldZoomValue=zoomValue.zoom;var firstElemStyle=document.firstElementChild.style;if(Math.abs(oldZoomValue-1)<.001)firstElemStyle.zoom="normal";else firstElemStyle.zoom=1/oldZoomValue}})(window);
