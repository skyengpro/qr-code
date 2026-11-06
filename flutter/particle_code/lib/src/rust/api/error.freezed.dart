// GENERATED CODE - DO NOT MODIFY BY HAND
// coverage:ignore-file
// ignore_for_file: type=lint, type=warning, deprecated_member_use, deprecated_member_use_from_same_package
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'error.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

// GENERATED CODE - DO NOT MODIFY BY HAND
// dart format off
T _$identity<T>(T value) => value;
/// @nodoc
mixin _$FfiError {

 String get field0;
/// Create a copy of FfiError
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$FfiErrorCopyWith<FfiError> get copyWith => _$FfiErrorCopyWithImpl<FfiError>(this as FfiError, _$identity);



@override
bool operator ==(Object other) {
  final _this = this as FfiError;
  return identical(this, other) || (other.runtimeType == runtimeType&&other is FfiError&&(identical(other.field0, _this.field0) || other.field0 == _this.field0));
}


@override
int get hashCode {
  final _this = this as FfiError;
  return Object.hash(runtimeType,_this.field0);
}

@override
String toString() {
  final _this = this as FfiError;
  return 'FfiError(field0: ${_this.field0})';
}


}

/// @nodoc
abstract mixin class $FfiErrorCopyWith<$Res>  {
  factory $FfiErrorCopyWith(FfiError value, $Res Function(FfiError) _then) = _$FfiErrorCopyWithImpl;
@useResult
$Res call({
 String field0
});




}
/// @nodoc
class _$FfiErrorCopyWithImpl<$Res>
    implements $FfiErrorCopyWith<$Res> {
  _$FfiErrorCopyWithImpl(this._self, this._then);

  final FfiError _self;
  final $Res Function(FfiError) _then;

/// Create a copy of FfiError
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') @override $Res call({Object? field0 = null,}) {
  return _then(_self.copyWith(
field0: null == field0 ? _self.field0 : field0 // ignore: cast_nullable_to_non_nullable
as String,
  ));
}

}


/// Adds pattern-matching-related methods to [FfiError].
extension FfiErrorPatterns on FfiError {
/// A variant of `map` that fallback to returning `orElse`.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case _:
///     return orElse();
/// }
/// ```

@optionalTypeArgs TResult maybeMap<TResult extends Object?>({TResult Function( FfiError_InvalidConfig value)?  invalidConfig,TResult Function( FfiError_Encode value)?  encode,TResult Function( FfiError_InvalidArgument value)?  invalidArgument,TResult Function( FfiError_Internal value)?  internal,required TResult orElse(),}){
final _that = this;
switch (_that) {
case FfiError_InvalidConfig() when invalidConfig != null:
return invalidConfig(_that);case FfiError_Encode() when encode != null:
return encode(_that);case FfiError_InvalidArgument() when invalidArgument != null:
return invalidArgument(_that);case FfiError_Internal() when internal != null:
return internal(_that);case _:
  return orElse();

}
}
/// A `switch`-like method, using callbacks.
///
/// Callbacks receives the raw object, upcasted.
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case final Subclass2 value:
///     return ...;
/// }
/// ```

@optionalTypeArgs TResult map<TResult extends Object?>({required TResult Function( FfiError_InvalidConfig value)  invalidConfig,required TResult Function( FfiError_Encode value)  encode,required TResult Function( FfiError_InvalidArgument value)  invalidArgument,required TResult Function( FfiError_Internal value)  internal,}){
final _that = this;
switch (_that) {
case FfiError_InvalidConfig():
return invalidConfig(_that);case FfiError_Encode():
return encode(_that);case FfiError_InvalidArgument():
return invalidArgument(_that);case FfiError_Internal():
return internal(_that);}
}
/// A variant of `map` that fallback to returning `null`.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case _:
///     return null;
/// }
/// ```

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>({TResult? Function( FfiError_InvalidConfig value)?  invalidConfig,TResult? Function( FfiError_Encode value)?  encode,TResult? Function( FfiError_InvalidArgument value)?  invalidArgument,TResult? Function( FfiError_Internal value)?  internal,}){
final _that = this;
switch (_that) {
case FfiError_InvalidConfig() when invalidConfig != null:
return invalidConfig(_that);case FfiError_Encode() when encode != null:
return encode(_that);case FfiError_InvalidArgument() when invalidArgument != null:
return invalidArgument(_that);case FfiError_Internal() when internal != null:
return internal(_that);case _:
  return null;

}
}
/// A variant of `when` that fallback to an `orElse` callback.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case _:
///     return orElse();
/// }
/// ```

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>({TResult Function( String field0)?  invalidConfig,TResult Function( String field0)?  encode,TResult Function( String field0)?  invalidArgument,TResult Function( String field0)?  internal,required TResult orElse(),}) {final _that = this;
switch (_that) {
case FfiError_InvalidConfig() when invalidConfig != null:
return invalidConfig(_that.field0);case FfiError_Encode() when encode != null:
return encode(_that.field0);case FfiError_InvalidArgument() when invalidArgument != null:
return invalidArgument(_that.field0);case FfiError_Internal() when internal != null:
return internal(_that.field0);case _:
  return orElse();

}
}
/// A `switch`-like method, using callbacks.
///
/// As opposed to `map`, this offers destructuring.
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case Subclass2(:final field2):
///     return ...;
/// }
/// ```

@optionalTypeArgs TResult when<TResult extends Object?>({required TResult Function( String field0)  invalidConfig,required TResult Function( String field0)  encode,required TResult Function( String field0)  invalidArgument,required TResult Function( String field0)  internal,}) {final _that = this;
switch (_that) {
case FfiError_InvalidConfig():
return invalidConfig(_that.field0);case FfiError_Encode():
return encode(_that.field0);case FfiError_InvalidArgument():
return invalidArgument(_that.field0);case FfiError_Internal():
return internal(_that.field0);}
}
/// A variant of `when` that fallback to returning `null`
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case _:
///     return null;
/// }
/// ```

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>({TResult? Function( String field0)?  invalidConfig,TResult? Function( String field0)?  encode,TResult? Function( String field0)?  invalidArgument,TResult? Function( String field0)?  internal,}) {final _that = this;
switch (_that) {
case FfiError_InvalidConfig() when invalidConfig != null:
return invalidConfig(_that.field0);case FfiError_Encode() when encode != null:
return encode(_that.field0);case FfiError_InvalidArgument() when invalidArgument != null:
return invalidArgument(_that.field0);case FfiError_Internal() when internal != null:
return internal(_that.field0);case _:
  return null;

}
}

}

/// @nodoc


class FfiError_InvalidConfig extends FfiError {
  const FfiError_InvalidConfig(this.field0): super._();
  

@override final  String field0;

/// Create a copy of FfiError
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$FfiError_InvalidConfigCopyWith<FfiError_InvalidConfig> get copyWith => _$FfiError_InvalidConfigCopyWithImpl<FfiError_InvalidConfig>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is FfiError_InvalidConfig&&(identical(other.field0, field0) || other.field0 == field0));
}


@override
int get hashCode {
    return Object.hash(runtimeType,field0);
}

@override
String toString() {
    return 'FfiError.invalidConfig(field0: $field0)';
}


}

/// @nodoc
abstract mixin class $FfiError_InvalidConfigCopyWith<$Res> implements $FfiErrorCopyWith<$Res> {
  factory $FfiError_InvalidConfigCopyWith(FfiError_InvalidConfig value, $Res Function(FfiError_InvalidConfig) _then) = _$FfiError_InvalidConfigCopyWithImpl;
@override @useResult
$Res call({
 String field0
});




}
/// @nodoc
class _$FfiError_InvalidConfigCopyWithImpl<$Res>
    implements $FfiError_InvalidConfigCopyWith<$Res> {
  _$FfiError_InvalidConfigCopyWithImpl(this._self, this._then);

  final FfiError_InvalidConfig _self;
  final $Res Function(FfiError_InvalidConfig) _then;

/// Create a copy of FfiError
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? field0 = null,}) {
  return _then(FfiError_InvalidConfig(
null == field0 ? _self.field0 : field0 // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class FfiError_Encode extends FfiError {
  const FfiError_Encode(this.field0): super._();
  

@override final  String field0;

/// Create a copy of FfiError
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$FfiError_EncodeCopyWith<FfiError_Encode> get copyWith => _$FfiError_EncodeCopyWithImpl<FfiError_Encode>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is FfiError_Encode&&(identical(other.field0, field0) || other.field0 == field0));
}


@override
int get hashCode {
    return Object.hash(runtimeType,field0);
}

@override
String toString() {
    return 'FfiError.encode(field0: $field0)';
}


}

/// @nodoc
abstract mixin class $FfiError_EncodeCopyWith<$Res> implements $FfiErrorCopyWith<$Res> {
  factory $FfiError_EncodeCopyWith(FfiError_Encode value, $Res Function(FfiError_Encode) _then) = _$FfiError_EncodeCopyWithImpl;
@override @useResult
$Res call({
 String field0
});




}
/// @nodoc
class _$FfiError_EncodeCopyWithImpl<$Res>
    implements $FfiError_EncodeCopyWith<$Res> {
  _$FfiError_EncodeCopyWithImpl(this._self, this._then);

  final FfiError_Encode _self;
  final $Res Function(FfiError_Encode) _then;

/// Create a copy of FfiError
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? field0 = null,}) {
  return _then(FfiError_Encode(
null == field0 ? _self.field0 : field0 // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class FfiError_InvalidArgument extends FfiError {
  const FfiError_InvalidArgument(this.field0): super._();
  

@override final  String field0;

/// Create a copy of FfiError
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$FfiError_InvalidArgumentCopyWith<FfiError_InvalidArgument> get copyWith => _$FfiError_InvalidArgumentCopyWithImpl<FfiError_InvalidArgument>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is FfiError_InvalidArgument&&(identical(other.field0, field0) || other.field0 == field0));
}


@override
int get hashCode {
    return Object.hash(runtimeType,field0);
}

@override
String toString() {
    return 'FfiError.invalidArgument(field0: $field0)';
}


}

/// @nodoc
abstract mixin class $FfiError_InvalidArgumentCopyWith<$Res> implements $FfiErrorCopyWith<$Res> {
  factory $FfiError_InvalidArgumentCopyWith(FfiError_InvalidArgument value, $Res Function(FfiError_InvalidArgument) _then) = _$FfiError_InvalidArgumentCopyWithImpl;
@override @useResult
$Res call({
 String field0
});




}
/// @nodoc
class _$FfiError_InvalidArgumentCopyWithImpl<$Res>
    implements $FfiError_InvalidArgumentCopyWith<$Res> {
  _$FfiError_InvalidArgumentCopyWithImpl(this._self, this._then);

  final FfiError_InvalidArgument _self;
  final $Res Function(FfiError_InvalidArgument) _then;

/// Create a copy of FfiError
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? field0 = null,}) {
  return _then(FfiError_InvalidArgument(
null == field0 ? _self.field0 : field0 // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class FfiError_Internal extends FfiError {
  const FfiError_Internal(this.field0): super._();
  

@override final  String field0;

/// Create a copy of FfiError
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$FfiError_InternalCopyWith<FfiError_Internal> get copyWith => _$FfiError_InternalCopyWithImpl<FfiError_Internal>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is FfiError_Internal&&(identical(other.field0, field0) || other.field0 == field0));
}


@override
int get hashCode {
    return Object.hash(runtimeType,field0);
}

@override
String toString() {
    return 'FfiError.internal(field0: $field0)';
}


}

/// @nodoc
abstract mixin class $FfiError_InternalCopyWith<$Res> implements $FfiErrorCopyWith<$Res> {
  factory $FfiError_InternalCopyWith(FfiError_Internal value, $Res Function(FfiError_Internal) _then) = _$FfiError_InternalCopyWithImpl;
@override @useResult
$Res call({
 String field0
});




}
/// @nodoc
class _$FfiError_InternalCopyWithImpl<$Res>
    implements $FfiError_InternalCopyWith<$Res> {
  _$FfiError_InternalCopyWithImpl(this._self, this._then);

  final FfiError_Internal _self;
  final $Res Function(FfiError_Internal) _then;

/// Create a copy of FfiError
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? field0 = null,}) {
  return _then(FfiError_Internal(
null == field0 ? _self.field0 : field0 // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

// dart format on
